//! The SAT solver that picks one version per name: rules over package literals (`+id` install, `-id` don't),
//! unit propagation through two watched literals, conflict-driven learning, and the policy's preferences when
//! several packages could satisfy a requirement. A port of Composer's solver, so it decides as Composer decides.

use super::package::Link;
use super::policy::Policy;
use super::pool::{Pool, Request};
use crate::constraint::Constraint;
use std::collections::{HashMap, HashSet};

/// Why a rule exists.
#[derive(Debug, Clone, PartialEq)]
pub enum Reason {
    /// The root requires this name.
    RootRequire {
        name: String,
        constraint: Constraint,
    },
    /// This package (an arena index) must be installed as it is.
    Fixed(usize),
    /// A package (by arena index) conflicts with the link's target.
    Conflict { source: usize, link: Link },
    /// A package (by arena index) requires the link's target.
    Requires { source: usize, link: Link },
    /// Only one package of this name can be installed.
    SameName(String),
    /// Learned from a conflict; the index of the rules it came from.
    Learned(usize),
    /// An alias (arena index) needs the package it aliases.
    Alias(usize),
    /// A package (arena index) needs its alias when the alias is required.
    InverseAlias(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Generic,
    Two,
    /// At most one of the literals' packages (all negative) is installed.
    MultiConflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Package,
    Request,
    Learned,
}

#[derive(Debug, Clone)]
pub struct Rule {
    /// Sorted.
    pub literals: Vec<i32>,
    pub kind: Kind,
    pub reason: Reason,
    pub ty: Type,
    pub disabled: bool,
}

impl Rule {
    fn new(mut literals: Vec<i32>, kind: Kind, reason: Reason) -> Rule {
        literals.sort_unstable();
        Rule {
            literals,
            kind,
            reason,
            ty: Type::Package,
            disabled: false,
        }
    }

    pub fn is_assertion(&self) -> bool {
        self.kind == Kind::Generic && self.literals.len() == 1
    }

    /// The name a requirement-like rule is about, which the policy prefers same-vendor packages for.
    pub fn required_package<'a>(&'a self, pool: &'a Pool) -> Option<&'a str> {
        match &self.reason {
            Reason::RootRequire { name, .. } => Some(name),
            Reason::Fixed(p) => Some(&pool.arena[*p].name),
            Reason::Requires { link, .. } => Some(&link.target),
            _ => None,
        }
    }
}

/// The rules, in the order they were added; duplicates are dropped.
#[derive(Debug, Default)]
pub struct RuleSet {
    pub rules: Vec<Rule>,
    seen: HashSet<(u8, Vec<i32>)>,
}

impl RuleSet {
    pub fn add(&mut self, mut rule: Rule, ty: Type) -> bool {
        let tag = match rule.kind {
            Kind::Generic => 0,
            Kind::Two => 1,
            Kind::MultiConflict => 2,
        };
        if !self.seen.insert((tag, rule.literals.clone())) {
            return false;
        }
        rule.ty = ty;
        self.rules.push(rule);
        true
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Rule ids by type, in Composer's iteration order: package rules, then request rules, then learned ones.
    fn ordered(&self) -> Vec<usize> {
        let mut ids = vec![];
        for ty in [Type::Package, Type::Request, Type::Learned] {
            ids.extend((0..self.rules.len()).filter(|&i| self.rules[i].ty == ty));
        }
        ids
    }

    fn of_type(&self, ty: Type) -> Vec<usize> {
        (0..self.rules.len())
            .filter(|&i| self.rules[i].ty == ty)
            .collect()
    }
}

/// Generates the rules for a request (`RuleSetGenerator`).
pub struct Generator<'a> {
    pool: &'a Pool,
    ignored: &'a dyn Fn(&str) -> bool,
    rules: RuleSet,
    added: Vec<i32>,
    added_set: HashSet<i32>,
    by_name: indexmap::IndexMap<String, Vec<i32>>,
}

impl<'a> Generator<'a> {
    pub fn new(pool: &'a Pool, ignored: &'a dyn Fn(&str) -> bool) -> Generator<'a> {
        Generator {
            pool,
            ignored,
            rules: RuleSet::default(),
            added: vec![],
            added_set: HashSet::new(),
            by_name: indexmap::IndexMap::new(),
        }
    }

    fn require(&self, package: i32, providers: &[i32], reason: Reason) -> Option<Rule> {
        let mut literals = vec![-package];
        for &p in providers {
            if p == package {
                return None;
            }
            literals.push(p);
        }
        Some(Rule::new(literals, Kind::Generic, reason))
    }

    fn add(&mut self, ty: Type, rule: Option<Rule>) {
        if let Some(rule) = rule {
            self.rules.add(rule, ty);
        }
    }

    fn add_package(&mut self, id: i32) {
        let pool = self.pool;
        let mut queue = std::collections::VecDeque::from([id]);
        while let Some(id) = queue.pop_front() {
            if !self.added_set.insert(id) {
                continue;
            }
            self.added.push(id);
            let package = pool.package(id);
            if let Some(of) = package.alias_of {
                let index = pool.index(id);
                // A filter may have kept the aliased package out; then the alias cannot be installed either.
                let Some(of_id) = pool.id(of) else {
                    self.add(Type::Package, self.require(id, &[], Reason::Alias(index)));
                    continue;
                };
                queue.push_back(of_id);
                let rule = self.require(id, &[of_id], Reason::Alias(index));
                self.add(Type::Package, rule);
                let rule = self.require(of_id, &[id], Reason::InverseAlias(of));
                self.add(Type::Package, rule);
                if !package.self_version_requires {
                    continue;
                }
            } else {
                for name in package.names(false) {
                    self.by_name.entry(name.to_string()).or_default().push(id);
                }
            }
            for link in &package.requires {
                if (self.ignored)(&link.target) {
                    continue;
                }
                let providers = pool.what_provides(&link.target, Some(&link.constraint));
                let rule = self.require(
                    id,
                    &providers,
                    Reason::Requires {
                        source: pool.index(id),
                        link: link.clone(),
                    },
                );
                self.add(Type::Package, rule);
                queue.extend(providers.iter().copied());
            }
        }
    }

    fn add_conflicts(&mut self) {
        let pool = self.pool;
        for &id in &self.added.clone() {
            let package = pool.package(id);
            for link in &package.conflicts {
                if !self.by_name.contains_key(&link.target) || (self.ignored)(&link.target) {
                    continue;
                }
                for &conflict in pool
                    .what_provides(&link.target, Some(&link.constraint))
                    .iter()
                {
                    let other = pool.package(conflict);
                    if (other.alias_of.is_none() || other.name == link.target) && conflict != id {
                        let rule = Rule::new(
                            vec![-id, -conflict],
                            Kind::Two,
                            Reason::Conflict {
                                source: pool.index(id),
                                link: link.clone(),
                            },
                        );
                        self.rules.add(rule, Type::Package);
                    }
                }
            }
        }
        for (name, ids) in self.by_name.clone() {
            if ids.len() > 1 {
                let literals: Vec<i32> = ids.iter().map(|i| -i).collect();
                let kind = if literals.len() == 2 {
                    Kind::Two
                } else {
                    Kind::MultiConflict
                };
                self.rules.add(
                    Rule::new(literals, kind, Reason::SameName(name)),
                    Type::Package,
                );
            }
        }
    }

    pub fn rules_for(mut self, request: &Request) -> RuleSet {
        let pool = self.pool;
        for &p in &request.fixed {
            let Some(id) = pool.id(p) else {
                continue;
            };
            self.add_package(id);
            self.rules.add(
                Rule::new(vec![id], Kind::Generic, Reason::Fixed(p)),
                Type::Request,
            );
        }
        for (name, constraint) in &request.requires {
            if (self.ignored)(name) {
                continue;
            }
            let packages = pool.what_provides(name, Some(constraint));
            if packages.is_empty() {
                continue;
            }
            for &id in packages.iter() {
                self.add_package(id);
            }
            let rule = Rule::new(
                packages.to_vec(),
                Kind::Generic,
                Reason::RootRequire {
                    name: name.clone(),
                    constraint: constraint.clone(),
                },
            );
            self.rules.add(rule, Type::Request);
        }
        for id in 1..=pool.len() as i32 {
            let package = pool.package(id);
            if let Some(of) = package.alias_of
                && !self.added_set.contains(&id)
                && (package.root_alias || pool.id(of).is_some_and(|o| self.added_set.contains(&o)))
            {
                self.add_package(id);
            }
        }
        self.add_conflicts();
        self.rules
    }
}

/// The decisions so far: per package id the level it was decided at (negative: not installed), and the queue of
/// decisions with the rule behind each.
struct Decisions {
    map: Vec<i32>,
    queue: Vec<(i32, usize)>,
}

impl Decisions {
    fn new(packages: usize) -> Decisions {
        Decisions {
            map: vec![0; packages + 1],
            queue: vec![],
        }
    }

    fn at(&self, literal: i32) -> i32 {
        self.map[literal.unsigned_abs() as usize]
    }

    fn decide(&mut self, literal: i32, level: i32, why: usize) {
        debug_assert_eq!(self.at(literal), 0, "decided twice");
        self.map[literal.unsigned_abs() as usize] = if literal > 0 { level } else { -level };
        self.queue.push((literal, why));
    }

    fn satisfy(&self, literal: i32) -> bool {
        let d = self.at(literal);
        (literal > 0 && d > 0) || (literal < 0 && d < 0)
    }

    fn conflict(&self, literal: i32) -> bool {
        let d = self.at(literal);
        (d > 0 && literal < 0) || (d < 0 && literal > 0)
    }

    fn decided(&self, literal: i32) -> bool {
        self.at(literal) != 0
    }

    fn undecided(&self, literal: i32) -> bool {
        self.at(literal) == 0
    }

    fn decided_install(&self, literal: i32) -> bool {
        self.at(literal) > 0
    }

    fn level(&self, literal: i32) -> i32 {
        self.at(literal).abs()
    }

    fn rule_of(&self, literal: i32) -> usize {
        let id = literal.abs();
        (self.queue.iter())
            .find(|(l, _)| l.abs() == id)
            .map(|(_, r)| *r)
            .expect("a decided literal has a decision")
    }

    fn reset_to(&mut self, offset: isize) {
        while self.queue.len() as isize > offset + 1 {
            let (literal, _) = self.queue.pop().expect("non-empty");
            self.map[literal.unsigned_abs() as usize] = 0;
        }
    }

    fn revert_last(&mut self) {
        let (literal, _) = self.queue.pop().expect("non-empty");
        self.map[literal.unsigned_abs() as usize] = 0;
    }
}

/// A rule's two watched literals (every literal of a multi-conflict rule is watched).
struct Watch {
    rule: usize,
    w1: i32,
    w2: i32,
}

/// For each literal, the rules watching it, newest first (stored in reverse: the newest is last).
struct Watches {
    nodes: Vec<Watch>,
    chains: Vec<Vec<usize>>,
}

fn slot(literal: i32) -> usize {
    if literal > 0 {
        2 * literal as usize
    } else {
        2 * literal.unsigned_abs() as usize + 1
    }
}

impl Watches {
    fn new(packages: usize) -> Watches {
        Watches {
            nodes: vec![],
            chains: vec![vec![]; 2 * packages + 2],
        }
    }

    fn insert(&mut self, rules: &RuleSet, rule: usize, highest: Option<&Decisions>) {
        let r = &rules.rules[rule];
        if r.is_assertion() {
            return;
        }
        let w1 = r.literals.first().copied().unwrap_or(0);
        let mut w2 = r.literals.get(1).copied().unwrap_or(0);
        if let Some(decisions) = highest
            && r.literals.len() >= 3
            && r.kind != Kind::MultiConflict
        {
            let mut level = 0;
            for &l in &r.literals {
                if decisions.level(l) > level {
                    level = decisions.level(l);
                    w2 = l;
                }
            }
        }
        let node = self.nodes.len();
        self.nodes.push(Watch { rule, w1, w2 });
        if r.kind == Kind::MultiConflict {
            for &l in &r.literals {
                self.chains[slot(l)].push(node);
            }
        } else {
            self.chains[slot(w1)].push(node);
            self.chains[slot(w2)].push(node);
        }
    }

    /// Propagates the decision of `decided` at `level`; returns a conflicting rule.
    fn propagate(
        &mut self,
        rules: &RuleSet,
        decided: i32,
        level: i32,
        decisions: &mut Decisions,
    ) -> Option<usize> {
        let literal = -decided;
        let chain = slot(literal);
        let mut i = self.chains[chain].len();
        while i > 0 {
            i -= 1;
            let node = self.chains[chain][i];
            let rule_id = self.nodes[node].rule;
            let rule = &rules.rules[rule_id];
            if rule.kind != Kind::MultiConflict {
                let n = &self.nodes[node];
                let other = if n.w1 == literal { n.w2 } else { n.w1 };
                if !rule.disabled && !decisions.satisfy(other) {
                    let alternative = (rule.literals.iter())
                        .copied()
                        .find(|&l| l != literal && l != other && !decisions.conflict(l));
                    if let Some(to) = alternative {
                        let n = &mut self.nodes[node];
                        if n.w1 == literal {
                            n.w1 = to;
                        } else {
                            n.w2 = to;
                        }
                        self.chains[chain].remove(i);
                        self.chains[slot(to)].push(node);
                        continue;
                    }
                    if decisions.conflict(other) {
                        return Some(rule_id);
                    }
                    decisions.decide(other, level, rule_id);
                }
            } else {
                for &other in &rule.literals {
                    if other != literal && !decisions.satisfy(other) {
                        if decisions.conflict(other) {
                            return Some(rule_id);
                        }
                        decisions.decide(other, level, rule_id);
                    }
                }
            }
        }
        None
    }
}

/// A reason the request cannot be satisfied: the rules involved, in sections.
#[derive(Debug, Clone, Default)]
pub struct Problem {
    pub sections: Vec<Vec<Reason>>,
}

impl Problem {
    fn add(&mut self, reason: Reason) {
        if self.sections.is_empty() {
            self.sections.push(vec![]);
        }
        let last = self.sections.last_mut().expect("a section");
        if !last.contains(&reason) {
            last.push(reason);
        }
    }

    fn next_section(&mut self) {
        self.sections.push(vec![]);
    }

    pub fn reasons(&self) -> impl Iterator<Item = &Reason> {
        self.sections.iter().flatten()
    }
}

pub struct Solver<'a> {
    pool: &'a Pool,
    policy: &'a Policy,
    rules: RuleSet,
    watches: Watches,
    decisions: Decisions,
    fixed: HashSet<i32>,
    propagate_index: usize,
    branches: Vec<(Vec<i32>, i32)>,
    pub problems: Vec<Problem>,
    learned_pool: Vec<Vec<usize>>,
    learned_why: HashMap<usize, usize>,
}

/// The installed packages of a solution (arena indices) in Composer's order: the reverse of the decisions.
pub struct Solution {
    pub packages: Vec<usize>,
}

impl<'a> Solver<'a> {
    pub fn new(pool: &'a Pool, policy: &'a Policy) -> Solver<'a> {
        Solver {
            pool,
            policy,
            rules: RuleSet::default(),
            watches: Watches::new(pool.len()),
            decisions: Decisions::new(pool.len()),
            fixed: HashSet::new(),
            propagate_index: 0,
            branches: vec![],
            problems: vec![],
            learned_pool: vec![],
            learned_why: HashMap::new(),
        }
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Solves `request`; `ignored` names the platform requirements to ignore. On failure returns the problems.
    pub fn solve(
        &mut self,
        request: &Request,
        ignored: &dyn Fn(&str) -> bool,
    ) -> Result<Solution, Vec<Problem>> {
        self.fixed = request
            .fixed
            .iter()
            .filter_map(|&p| self.pool.id(p))
            .collect();
        self.rules = Generator::new(self.pool, ignored).rules_for(request);
        for (name, constraint) in &request.requires {
            if !ignored(name) && self.pool.what_provides(name, Some(constraint)).is_empty() {
                let mut problem = Problem::default();
                problem.add(Reason::RootRequire {
                    name: name.clone(),
                    constraint: constraint.clone(),
                });
                self.problems.push(problem);
            }
        }
        for id in self.rules.ordered() {
            self.watches.insert(&self.rules, id, None);
        }
        self.assertions();
        self.run();
        if !self.problems.is_empty() {
            return Err(std::mem::take(&mut self.problems));
        }
        let packages = (self.decisions.queue.iter().rev())
            .filter(|(l, _)| *l > 0)
            .map(|(l, _)| self.pool.index(*l))
            .collect();
        Ok(Solution { packages })
    }

    fn assertions(&mut self) {
        let start = self.decisions.queue.len() as isize - 1;
        let mut i = 0;
        while i < self.rules.len() {
            let rule = &self.rules.rules[i];
            if !rule.is_assertion() || rule.disabled {
                i += 1;
                continue;
            }
            let literal = rule.literals[0];
            if !self.decisions.decided(literal) {
                self.decisions.decide(literal, 1, i);
                i += 1;
                continue;
            }
            if self.decisions.satisfy(literal) {
                i += 1;
                continue;
            }
            if rule.ty == Type::Learned {
                self.rules.rules[i].disabled = true;
                i += 1;
                continue;
            }
            let conflict = self.decisions.rule_of(literal);
            let mut problem = Problem::default();
            problem.add(self.rules.rules[i].reason.clone());
            problem.add(self.rules.rules[conflict].reason.clone());
            if self.rules.rules[conflict].ty == Type::Package {
                self.rules.rules[i].disabled = true;
                self.problems.push(problem);
                i += 1;
                continue;
            }
            for r in self.rules.of_type(Type::Request) {
                let assert = &self.rules.rules[r];
                if assert.disabled
                    || !assert.is_assertion()
                    || assert.literals[0].abs() != literal.abs()
                {
                    continue;
                }
                problem.add(assert.reason.clone());
                self.rules.rules[r].disabled = true;
            }
            self.problems.push(problem);
            self.decisions.reset_to(start);
            i = 0;
        }
    }

    fn propagate(&mut self, level: i32) -> Option<usize> {
        while self.propagate_index < self.decisions.queue.len() {
            let literal = self.decisions.queue[self.propagate_index].0;
            let conflict = self
                .watches
                .propagate(&self.rules, literal, level, &mut self.decisions);
            self.propagate_index += 1;
            if conflict.is_some() {
                return conflict;
            }
        }
        None
    }

    fn revert(&mut self, level: i32) {
        while let Some(&(literal, _)) = self.decisions.queue.last() {
            if self.decisions.undecided(literal) || self.decisions.level(literal) <= level {
                break;
            }
            self.decisions.revert_last();
            self.propagate_index = self.decisions.queue.len();
        }
        while self.branches.last().is_some_and(|(_, l)| *l >= level) {
            self.branches.pop();
        }
    }

    fn set_propagate_learn(&mut self, mut level: i32, literal: i32, rule: usize) -> i32 {
        level += 1;
        self.decisions.decide(literal, level, rule);
        while let Some(conflict) = self.propagate(level) {
            if level == 1 {
                self.analyze_unsolvable(conflict);
                return 0;
            }
            let (learn, new_level, new_rule, why) = self.analyze(level, conflict);
            assert!(
                new_level > 0 && new_level < level,
                "solver reverting to invalid level {new_level} from {level}"
            );
            level = new_level;
            self.revert(level);
            let literals = new_rule.literals.clone();
            let id = if self.rules.add(new_rule, Type::Learned) {
                self.rules.len() - 1
            } else {
                (self.rules.rules.iter())
                    .position(|r| r.kind == Kind::Generic && r.literals == literals)
                    .expect("the duplicate is in the set")
            };
            self.learned_why.insert(id, why);
            self.watches.insert(&self.rules, id, Some(&self.decisions));
            self.decisions.decide(learn, level, id);
        }
        level
    }

    fn select_and_install(&mut self, level: i32, queue: Vec<i32>, rule: usize) -> i32 {
        let required = self.rules.rules[rule]
            .required_package(self.pool)
            .map(String::from);
        let mut literals = self
            .policy
            .select_preferred(self.pool, queue, required.as_deref());
        let selected = literals.remove(0);
        if !literals.is_empty() {
            self.branches.push((literals, level));
        }
        self.set_propagate_learn(level, selected, rule)
    }

    fn analyze(&mut self, level: i32, conflict: usize) -> (i32, i32, Rule, usize) {
        let mut rule = conflict;
        let mut rule_level = 1;
        let mut num = 0;
        let mut l1num = 0;
        let mut seen: HashSet<i32> = HashSet::new();
        let mut learned: Option<i32> = None;
        let mut others: Vec<i32> = vec![];
        let mut decision_id = self.decisions.queue.len();
        self.learned_pool.push(vec![]);
        'outer: loop {
            self.learned_pool.last_mut().expect("pushed").push(rule);
            let r = &self.rules.rules[rule];
            for &literal in &r.literals {
                if r.kind == Kind::MultiConflict && !self.decisions.decided(literal) {
                    continue;
                }
                if self.decisions.satisfy(literal) || !seen.insert(literal.abs()) {
                    continue;
                }
                let l = self.decisions.level(literal);
                if l == 1 {
                    l1num += 1;
                } else if l == level {
                    num += 1;
                } else {
                    others.push(literal);
                    rule_level = rule_level.max(l);
                }
            }
            let mut retry = true;
            while retry {
                retry = false;
                if num == 0 {
                    l1num -= 1;
                    if l1num == 0 {
                        break 'outer;
                    }
                }
                let mut literal;
                loop {
                    assert!(decision_id > 0, "solver reached an invalid decision id");
                    decision_id -= 1;
                    literal = self.decisions.queue[decision_id].0;
                    if seen.contains(&literal.abs()) {
                        break;
                    }
                }
                seen.remove(&literal.abs());
                if num != 0 {
                    num -= 1;
                    if num == 0 {
                        learned = Some(-literal);
                        if l1num == 0 {
                            break 'outer;
                        }
                        for o in &others {
                            seen.remove(&o.abs());
                        }
                        l1num += 1;
                        retry = true;
                        continue;
                    }
                }
                let why = self.decisions.queue[decision_id].1;
                if self.rules.rules[why].kind == Kind::MultiConflict {
                    for &rl in &self.rules.rules[why].literals {
                        if !seen.contains(&rl.abs()) && self.decisions.satisfy(-rl) {
                            self.learned_pool.last_mut().expect("pushed").push(why);
                            let l = self.decisions.level(rl);
                            if l == 1 {
                                l1num += 1;
                            } else if l == level {
                                num += 1;
                            } else {
                                others.push(rl);
                                rule_level = rule_level.max(l);
                            }
                            seen.insert(rl.abs());
                            break;
                        }
                    }
                    retry = true;
                }
            }
            rule = self.decisions.queue[decision_id].1;
        }
        let why = self.learned_pool.len() - 1;
        let learned = learned.expect("a learnable literal");
        others.insert(0, learned);
        let rule = Rule::new(others, Kind::Generic, Reason::Learned(why));
        (learned, rule_level, rule, why)
    }

    fn unsolvable_rule(&self, problem: &mut Problem, rule: usize, seen: &mut HashSet<usize>) {
        seen.insert(rule);
        let r = &self.rules.rules[rule];
        if r.ty == Type::Learned {
            let why = self.learned_why[&rule];
            for &p in &self.learned_pool[why] {
                if !seen.contains(&p) {
                    self.unsolvable_rule(problem, p, seen);
                }
            }
            return;
        }
        if r.ty == Type::Package {
            return;
        }
        problem.next_section();
        problem.add(r.reason.clone());
    }

    fn analyze_unsolvable(&mut self, conflict: usize) {
        let mut problem = Problem::default();
        problem.add(self.rules.rules[conflict].reason.clone());
        let mut rule_seen = HashSet::new();
        self.unsolvable_rule(&mut problem, conflict, &mut rule_seen);
        let mut seen: HashSet<i32> = HashSet::new();
        for &l in &self.rules.rules[conflict].literals {
            if !self.decisions.satisfy(l) {
                seen.insert(l.abs());
            }
        }
        for &(literal, why) in self.decisions.queue.iter().rev() {
            if !seen.contains(&literal.abs()) {
                continue;
            }
            problem.add(self.rules.rules[why].reason.clone());
            self.unsolvable_rule(&mut problem, why, &mut rule_seen);
            for &l in &self.rules.rules[why].literals {
                if !self.decisions.satisfy(l) {
                    seen.insert(l.abs());
                }
            }
        }
        self.problems.push(problem);
    }

    fn run(&mut self) {
        self.propagate_index = 0;
        let mut level = 1;
        let mut system_level = level + 1;
        loop {
            if level == 1
                && let Some(conflict) = self.propagate(level)
            {
                self.analyze_unsolvable(conflict);
                return;
            }
            if level < system_level {
                let requests = self.rules.of_type(Type::Request);
                let mut stopped_at = None;
                for (k, &id) in requests.iter().enumerate() {
                    let rule = &self.rules.rules[id];
                    if rule.disabled {
                        continue;
                    }
                    let mut queue = vec![];
                    let mut none_satisfied = true;
                    for &l in &rule.literals {
                        if self.decisions.satisfy(l) {
                            none_satisfied = false;
                            break;
                        }
                        if l > 0 && self.decisions.undecided(l) {
                            queue.push(l);
                        }
                    }
                    if none_satisfied && !queue.is_empty() {
                        let pruned: Vec<i32> = queue
                            .iter()
                            .copied()
                            .filter(|l| self.fixed.contains(&l.abs()))
                            .collect();
                        if !pruned.is_empty() {
                            queue = pruned;
                        }
                        let old = level;
                        level = self.select_and_install(level, queue, id);
                        if level == 0 {
                            return;
                        }
                        if level <= old {
                            stopped_at = Some(k);
                            break;
                        }
                    }
                }
                system_level = level + 1;
                if stopped_at.is_some_and(|k| k + 1 < requests.len()) {
                    continue;
                }
            }
            if level < system_level {
                system_level = level;
            }
            let mut count = self.rules.len();
            let (mut i, mut n) = (0usize, 0isize);
            while n < count as isize {
                if i == count {
                    i = 0;
                }
                let rule = &self.rules.rules[i];
                let mut queue = vec![];
                let mut skip = rule.disabled;
                if !skip {
                    for &l in &rule.literals {
                        if l <= 0 {
                            if !self.decisions.decided_install(l) {
                                skip = true;
                                break;
                            }
                        } else {
                            if self.decisions.decided_install(l) {
                                skip = true;
                                break;
                            }
                            if self.decisions.undecided(l) {
                                queue.push(l);
                            }
                        }
                    }
                }
                if !skip && queue.len() >= 2 {
                    level = self.select_and_install(level, queue, i);
                    if level == 0 {
                        return;
                    }
                    count = self.rules.len();
                    n = -1;
                }
                i += 1;
                n += 1;
            }
            if level < system_level {
                continue;
            }
            let mut last: Option<(i32, usize, usize, i32)> = None;
            for b in (0..self.branches.len()).rev() {
                let (literals, l) = &self.branches[b];
                for (offset, &literal) in literals.iter().enumerate() {
                    if literal > 0 && self.decisions.level(literal) > l + 1 {
                        last = Some((literal, b, offset, *l));
                    }
                }
            }
            if let Some((literal, b, offset, l)) = last {
                self.branches[b].0.remove(offset);
                level = l;
                self.revert(level);
                let why = self.decisions.queue.last().expect("decisions remain").1;
                level = self.set_propagate_learn(level, literal, why);
                if level == 0 {
                    return;
                }
                continue;
            }
            break;
        }
    }
}
