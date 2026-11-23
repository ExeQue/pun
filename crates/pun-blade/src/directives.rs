//! Directive knowledge: the lexer's accept-all set (`lexer/directives.ts`) and the tree builder's
//! structural registry (`tree/directives.ts`), with the bundled directive definitions.

use std::collections::{HashMap, HashSet};

/// The lexer's directive filter. Pun's parse path always tokenizes with accept-all, as the plugin's
/// `parse()` does (it passes no directive set to `tokenize`).
pub(crate) struct Directives;

impl Directives {
    pub(crate) fn accept_all() -> Self {
        Directives
    }

    pub(crate) fn accepts_all(&self) -> bool {
        true
    }

    pub(crate) fn is_directive_lower(&self, _name: &str) -> bool {
        true
    }
}

const DEFAULT_DIRECTIVES: &[&str] = &[
    "if", "elseif", "else", "endif", "unless", "endunless", "isset", "endisset", "empty", "endempty", "switch",
    "case", "break", "default", "endswitch", "foreach", "endforeach", "for", "endfor", "while", "endwhile",
    "forelse", "endforelse", "continue", "auth", "elseauth", "endauth", "guest", "elseguest", "endguest", "can",
    "elsecan", "endcan", "canany", "elsecanany", "endcanany", "cannot", "elsecannot", "endcannot", "env",
    "elseenv", "endenv", "production", "elseproduction", "endproduction", "section", "endsection", "yield",
    "show", "stop", "append", "overwrite", "extends", "extendsFirst", "parent", "hasSection", "sectionMissing",
    "endhasSection", "endsectionMissing", "include", "includeIf", "includeWhen", "includeUnless",
    "includeFirst", "includeIsolated", "each", "once", "endonce", "push", "endpush", "pushOnce", "endPushOnce",
    "pushIf", "elsePushIf", "elsePush", "endPushIf", "prepend", "endprepend", "prependOnce", "endPrependOnce",
    "stack", "hasStack", "component", "endcomponent", "endComponentClass", "componentFirst",
    "endComponentFirst", "slot", "endslot", "props", "aware", "csrf", "method", "error", "enderror", "old",
    "inject", "dd", "dump", "vite", "viteReactRefresh", "fonts", "json", "js", "unset", "class", "style",
    "checked", "selected", "disabled", "readonly", "required", "bool", "php", "endphp", "verbatim",
    "endverbatim", "fragment", "endfragment", "session", "endsession", "context", "endcontext", "lang",
    "endlang", "choice", "livewire", "livewireStyles", "livewireScripts", "entangle", "this", "persist",
    "endpersist", "teleport", "endteleport", "volt", "inertia", "inertiaHead", "filamentStyles",
    "filamentScripts", "svg", "role", "endrole", "hasrole", "endhasrole", "hasanyrole", "endhasanyrole",
    "hasallroles", "endhasallroles", "unlessrole", "endunlessrole", "feature", "endfeature", "featureany",
    "endfeatureany", "paddleJS", "use",
];

/// `getCanonicalDirectiveName`: the default directive list's spelling of a name, if it is known.
pub(crate) fn canonical_directive_name(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    DEFAULT_DIRECTIVES.iter().copied().find(|d| d.to_ascii_lowercase() == lower)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    None,
    Opening,
    Closing,
    Mixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArgReq {
    Optional,
    Required,
    NotAllowed,
}

#[derive(Debug, Clone)]
pub(crate) struct Directive {
    pub(crate) name: String,
    pub(crate) args: ArgReq,
    pub(crate) role: Role,
    pub(crate) is_condition: bool,
    pub(crate) terminators: Vec<String>,
    pub(crate) condition_like_branches: Vec<String>,
    pub(crate) terminator: Option<String>,
    pub(crate) is_switch: bool,
    pub(crate) switch_parent: Option<String>,
    pub(crate) is_switch_branch: bool,
    pub(crate) is_switch_terminator: bool,
    pub(crate) is_conditional_pair: bool,
    pub(crate) pairing_strategy: Option<String>,
    pub(crate) is_conditional_close: bool,
}

impl Directive {
    fn new(name: &str) -> Self {
        Directive {
            name: name.to_string(),
            args: ArgReq::Optional,
            role: Role::None,
            is_condition: false,
            terminators: Vec::new(),
            condition_like_branches: Vec::new(),
            terminator: None,
            is_switch: false,
            switch_parent: None,
            is_switch_branch: false,
            is_switch_terminator: false,
            is_conditional_pair: false,
            pairing_strategy: None,
            is_conditional_close: false,
        }
    }
}

/// One bundled definition: names, args, and the `structure` fields.
struct Def {
    name: &'static str,
    args: ArgReq,
    role: &'static str,
    condition: bool,
    terminators: &'static str,
    branches: &'static str,
    kind: &'static str,
    parent: &'static str,
    pairing: &'static str,
}

const fn d(name: &'static str, args: ArgReq) -> Def {
    Def { name, args, role: "", condition: false, terminators: "", branches: "", kind: "", parent: "", pairing: "" }
}

const fn s(name: &'static str, args: ArgReq, role: &'static str, condition: bool, terminators: &'static str) -> Def {
    Def { name, args, role, condition, terminators, branches: "", kind: "", parent: "", pairing: "" }
}

use ArgReq::{NotAllowed as NO, Optional as OPT, Required as REQ};

#[rustfmt::skip]
const BUNDLED: &[Def] = &[
    // conditions.json
    s("if", REQ, "open", true, "elseif,endif"),
    s("elseif", REQ, "mixed", false, "elseif,endif"),
    s("else", NO, "mixed", false, "endif"),
    s("endif", NO, "close", true, ""),
    s("unless", REQ, "open", true, "elseif,endunless"),
    s("endunless", NO, "close", true, ""),
    s("isset", REQ, "open", true, "elseif,endIsset"),
    s("endIsset", NO, "close", true, ""),
    Def { branches: "else", ..s("empty", REQ, "open", false, "endempty") },
    s("endempty", NO, "close", false, ""),
    s("once", OPT, "open", true, "elseif,endif,endOnce"),
    s("endOnce", NO, "close", true, ""),
    s("pushIf", REQ, "open", true, "elsePushIf,elsePush,elseif,endPushIf"),
    s("elsePushIf", REQ, "mixed", true, "elsePushIf,elsePush,elseif,endPushIf"),
    s("elsePush", REQ, "mixed", true, "endPushIf"),
    s("endPushIf", NO, "close", true, ""),
    s("hasStack", REQ, "open", true, "elseif,endif"),
    // auth.json
    s("auth", OPT, "open", true, "elseauth,endauth"),
    s("elseauth", OPT, "mixed", true, "elseauth,endauth"),
    s("endauth", NO, "close", true, ""),
    // guest.json
    s("guest", OPT, "open", true, "elseguest,endguest"),
    s("elseguest", OPT, "mixed", true, "elseguest,endguest"),
    s("endguest", NO, "close", true, ""),
    // authorizations.json
    s("can", REQ, "open", true, "elsecan,endcan"),
    s("elsecan", REQ, "mixed", true, "elsecan,endcan"),
    s("endcan", NO, "close", true, ""),
    s("cannot", REQ, "open", true, "elsecannot,endcannot"),
    s("elsecannot", REQ, "mixed", true, "elsecannot,endcannot"),
    s("endcannot", NO, "close", true, ""),
    s("canany", REQ, "open", true, "elsecanany,endcanany"),
    s("elsecanany", REQ, "mixed", true, "elsecanany,endcanany"),
    s("endcanany", NO, "close", true, ""),
    // env.json
    s("env", REQ, "open", true, "elseenv,endenv"),
    s("elseenv", REQ, "mixed", true, ""),
    s("endenv", NO, "close", true, ""),
    s("production", NO, "open", true, "elseproduction,endproduction"),
    s("elseproduction", NO, "mixed", true, ""),
    s("endproduction", NO, "close", true, ""),
    // for.json, foreach.json, forelse.json, while.json
    s("for", REQ, "open", false, "endfor"),
    s("endfor", NO, "close", false, ""),
    s("foreach", REQ, "open", false, "endforeach"),
    s("endforeach", NO, "close", false, ""),
    Def { branches: "empty", ..s("forelse", REQ, "open", false, "endforelse") },
    s("endforelse", NO, "close", false, ""),
    s("while", REQ, "open", false, "endwhile"),
    s("endwhile", NO, "close", false, ""),
    // switch.json
    Def { kind: "switch", ..s("switch", REQ, "open", false, "") },
    Def { parent: "switch", ..s("case", REQ, "branch", false, "") },
    Def { parent: "switch", ..s("default", NO, "branch", false, "") },
    Def { parent: "switch", ..s("break", OPT, "branch_terminator", false, "") },
    s("endSwitch", NO, "close", false, ""),
    // error.json
    Def { branches: "else", ..s("error", REQ, "open", false, "enderror") },
    s("enderror", NO, "close", false, ""),
    // components.json
    d("props,aware", REQ),
    s("component", REQ, "open", false, "endComponent,endComponentClass"),
    s("endComponent,endComponentClass,endSlot,endComponentFirst", NO, "close", false, ""),
    s("slot", REQ, "open", false, "endSlot"),
    s("componentFirst", REQ, "open", false, "endComponentFirst"),
    // layouts.json
    d("extends,extendsFirst,yield", REQ),
    d("parent", NO),
    Def { pairing: "section_style", ..s("section", REQ, "conditional_pair", false, "show,append,overwrite,stop,endsection") },
    s("show,append,overwrite,stop,endsection", NO, "conditional_close", false, ""),
    s("hasSection,sectionMissing", REQ, "open", true, "elseif,else,endif,endhasSection,endsectionMissing"),
    s("endhasSection,endsectionMissing", NO, "close", true, ""),
    // stacks.json
    d("stack", REQ),
    s("push", REQ, "open", false, "endpush,endpushOnce"),
    s("endpush,endpushOnce,endprepend,endprependOnce", NO, "close", false, ""),
    s("pushOnce", REQ, "open", false, "endpush,endpushOnce"),
    s("prepend", REQ, "open", false, "endprepend,endprependOnce"),
    s("prependOnce", REQ, "open", false, "endprepend,endprependOnce"),
    // fragments.json, sessions.json, contexts.json
    s("fragment", REQ, "open", false, "endfragment"),
    s("endfragment", NO, "close", false, ""),
    s("session", REQ, "open", false, "endsession"),
    s("endsession", NO, "close", false, ""),
    s("context", REQ, "open", false, "endcontext"),
    s("endcontext", NO, "close", false, ""),
    // livewire.json
    s("persist", REQ, "open", false, "endpersist"),
    s("endpersist", NO, "close", false, ""),
    s("teleport", REQ, "open", false, "endteleport"),
    s("endteleport", NO, "close", false, ""),
    d("livewire", REQ),
    d("livewireStyles,livewireScripts", NO),
    d("entangle", REQ),
    d("this", OPT),
    d("volt", OPT),
    // spatie-permission.json
    s("role", REQ, "open", true, "elserole,endrole"),
    s("endrole", NO, "close", true, ""),
    s("hasrole", REQ, "open", true, "elsehasrole,endhasrole"),
    s("endhasrole", NO, "close", true, ""),
    s("hasanyrole", REQ, "open", true, "elsehasanyrole,endhasanyrole"),
    s("endhasanyrole", NO, "close", true, ""),
    s("hasallroles", REQ, "open", true, "elsehasallroles,endhasallroles"),
    s("endhasallroles", NO, "close", true, ""),
    s("unlessrole", REQ, "open", true, "elseunlessrole,endunlessrole"),
    s("endunlessrole", NO, "close", true, ""),
    // pennant.json
    s("feature", REQ, "open", true, "elsefeature,endfeature"),
    s("endfeature", NO, "close", true, ""),
    s("featureany", REQ, "open", true, "elsefeatureany,endfeatureany"),
    s("endfeatureany", NO, "close", true, ""),
    // translations.json
    Def { pairing: "lang_style", ..s("lang", OPT, "conditional_pair", false, "endlang") },
    d("choice", REQ),
    s("endlang", NO, "conditional_close", false, ""),
    // verbatim.json
    s("verbatim", NO, "open", false, "endverbatim"),
    s("endverbatim", NO, "close", false, ""),
    // includes.json
    d("each", REQ), d("include", REQ), d("includeIf", REQ), d("includeWhen", REQ), d("includeUnless", REQ),
    d("includeFirst", REQ), d("includeIsolated", REQ),
    // basic.json
    d("csrf", NO), d("method", REQ), d("inject", REQ), d("dd", REQ), d("dump", REQ), d("js", REQ), d("json", REQ),
    d("vite", OPT), d("viteReactRefresh", NO), d("fonts", OPT), d("bool", REQ), d("class", REQ), d("style", REQ),
    d("selected", REQ), d("checked", REQ), d("disabled", REQ), d("required", REQ), d("readonly", REQ),
    d("old", REQ), d("use", REQ), d("unset", REQ), d("inertia", REQ), d("inertiaHead", OPT),
    d("filamentStyles,filamentScripts", NO), d("svg", REQ), d("paddleJS", NO), d("continue", OPT),
    s("php", OPT, "open", false, "endphp"),
    s("endphp", NO, "close", false, ""),
    // The default `statamic` syntax plugin's tree directives.
    s("antlers", NO, "open", false, "endantlers"),
    s("endantlers", NO, "close", false, ""),
];

fn parse_list(value: &str) -> Vec<String> {
    value.split(',').map(|s| s.trim().to_ascii_lowercase()).filter(|s| !s.is_empty()).collect()
}

/// The tree builder's directive registry. `order` keeps JS `Map` insertion order, which the
/// opener and condition-branch lookups iterate in.
#[derive(Clone)]
pub(crate) struct TreeDirectives {
    order: Vec<String>,
    map: HashMap<String, Directive>,
    conditions: HashSet<String>,
    final_terminators: HashSet<String>,
    seen: HashSet<String>,
    advisory_pairs: HashSet<String>,
    advisory_conditions: HashSet<String>,
    condition_branches: Vec<String>,
    condition_terminators: Vec<String>,
}

impl TreeDirectives {
    pub(crate) fn with_defaults() -> Self {
        let mut t = TreeDirectives {
            order: Vec::new(),
            map: HashMap::new(),
            conditions: HashSet::new(),
            final_terminators: HashSet::new(),
            seen: HashSet::new(),
            advisory_pairs: HashSet::new(),
            advisory_conditions: HashSet::new(),
            condition_branches: Vec::new(),
            condition_terminators: Vec::new(),
        };
        for def in BUNDLED {
            let role = match def.role {
                "open" | "conditional_pair" => Role::Opening,
                "close" | "conditional_close" => Role::Closing,
                "mixed" => Role::Mixed,
                _ => Role::None,
            };
            let terminators = parse_list(def.terminators);
            let branches = parse_list(def.branches);
            let switch_parent = if def.parent.is_empty() { None } else { Some(def.parent.to_ascii_lowercase()) };
            for name in def.name.split(',').map(str::trim).filter(|n| !n.is_empty()) {
                let mut dir = Directive::new(name);
                dir.args = def.args;
                dir.role = role;
                dir.is_condition = def.condition;
                dir.terminators.clone_from(&terminators);
                dir.condition_like_branches.clone_from(&branches);
                dir.terminator = terminators.last().cloned();
                dir.is_switch = def.kind == "switch";
                dir.switch_parent.clone_from(&switch_parent);
                dir.is_switch_branch = def.role == "branch" && switch_parent.as_deref() == Some("switch");
                dir.is_switch_terminator =
                    def.role == "branch_terminator" && switch_parent.as_deref() == Some("switch");
                dir.is_conditional_pair = def.role == "conditional_pair";
                if !def.pairing.is_empty() {
                    dir.pairing_strategy = Some(def.pairing.to_string());
                }
                dir.is_conditional_close = def.role == "conditional_close";
                t.add(dir);
            }
        }
        t.recompute();
        t
    }

    fn add(&mut self, dir: Directive) {
        let name = dir.name.to_ascii_lowercase();
        if dir.role == Role::Closing && dir.terminators.is_empty() {
            self.final_terminators.insert(name.clone());
        }
        if dir.is_condition {
            self.conditions.insert(name.clone());
        }
        if !self.map.contains_key(&name) {
            self.order.push(name.clone());
        }
        self.map.insert(name, dir);
    }

    fn recompute(&mut self) {
        let mut branches = vec!["else".to_string()];
        let mut terminators = Vec::new();
        for name in &self.order {
            let d = &self.map[name];
            if d.is_condition && d.role == Role::Opening {
                branches.extend(d.terminators.iter().cloned());
            }
            if d.is_condition && d.role == Role::Closing {
                terminators.push(d.name.to_ascii_lowercase());
            }
        }
        self.condition_branches = branches;
        self.condition_terminators = terminators;
    }

    pub(crate) fn is_directive(&self, name: &str) -> bool {
        self.map.contains_key(&name.to_ascii_lowercase())
    }

    pub(crate) fn is_condition(&self, name: &str) -> bool {
        self.conditions.contains(&name.to_ascii_lowercase())
    }

    pub(crate) fn get(&self, name: &str) -> Option<&Directive> {
        self.map.get(&name.to_ascii_lowercase())
    }

    pub(crate) fn is_paired(&self, name: &str) -> bool {
        self.get(name)
            .is_some_and(|d| d.role == Role::Opening && !d.terminators.is_empty() && d.terminator.is_some())
    }

    pub(crate) fn is_switch(&self, name: &str) -> bool {
        self.get(name).is_some_and(|d| d.is_switch)
    }

    pub(crate) fn is_switch_branch(&self, name: &str) -> bool {
        self.get(name).is_some_and(|d| d.is_switch_branch)
    }

    pub(crate) fn is_switch_terminator(&self, name: &str) -> bool {
        self.get(name).is_some_and(|d| d.is_switch_terminator)
    }

    pub(crate) fn is_conditional_pair(&self, name: &str) -> bool {
        self.get(name).is_some_and(|d| d.is_conditional_pair)
    }

    pub(crate) fn terminators(&self, name: &str) -> Vec<String> {
        self.get(name).map(|d| d.terminators.clone()).unwrap_or_default()
    }

    pub(crate) fn openers_for_terminators(&self, terminators: &[String]) -> Vec<String> {
        if terminators.is_empty() {
            return Vec::new();
        }
        let set: HashSet<String> = terminators.iter().map(|t| t.to_ascii_lowercase()).collect();
        let mut out = Vec::new();
        for name in &self.order {
            let d = &self.map[name];
            if d.role != Role::Opening {
                continue;
            }
            if d.terminators.iter().any(|t| set.contains(t)) {
                out.push(d.name.to_ascii_lowercase());
            }
        }
        out
    }

    pub(crate) fn branches(&self, name: &str) -> Vec<String> {
        match self.get(name) {
            Some(d) if d.is_condition => self.condition_branches.clone(),
            _ => Vec::new(),
        }
    }

    pub(crate) fn switch_branches(&self, switch_name: &str) -> Vec<String> {
        let lower = switch_name.to_ascii_lowercase();
        let mut out = Vec::new();
        for name in &self.order {
            let d = &self.map[name];
            if d.switch_parent.as_deref() == Some(lower.as_str()) {
                out.push(d.name.clone());
            }
        }
        out
    }

    pub(crate) fn condition_terminators(&self) -> &[String] {
        &self.condition_terminators
    }

    pub(crate) fn pairing_strategy(&self, name: &str) -> Option<&str> {
        self.get(name).and_then(|d| d.pairing_strategy.as_deref())
    }

    pub(crate) fn has_seen(&self, name: &str) -> bool {
        self.seen.contains(&name.to_ascii_lowercase())
    }

    pub(crate) fn has_advisory_pair(&self, name: &str) -> bool {
        self.advisory_pairs.contains(&name.to_ascii_lowercase())
    }

    pub(crate) fn has_advisory_condition(&self, name: &str) -> bool {
        self.advisory_conditions.contains(&name.to_ascii_lowercase())
    }

    /// `train`: remember every directive name in the token stream and the custom `@x`/`@endx`
    /// and `@x`/`@elsex` families among them.
    pub(crate) fn train(&mut self, names: &[String]) {
        let mut found: Vec<String> = Vec::new();
        let mut found_set: HashSet<String> = HashSet::new();
        for name in names {
            self.seen.insert(name.clone());
            if found_set.insert(name.clone()) {
                found.push(name.clone());
            }
        }
        for name in &found {
            if self.map.contains_key(name) {
                continue;
            }
            if found_set.contains(&format!("end{name}")) {
                self.advisory_pairs.insert(name.clone());
            }
            if found_set.contains(&format!("else{name}")) {
                self.advisory_conditions.insert(name.clone());
            }
        }
    }
}
