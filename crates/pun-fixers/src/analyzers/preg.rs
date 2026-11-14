use pcre2_sys::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{c_int, c_void};
use std::hash::{BuildHasherDefault, Hasher};
use std::rc::Rc;

pub(crate) const PREG_SPLIT_NO_EMPTY: u32 = 1;
pub(crate) const PREG_SPLIT_DELIM_CAPTURE: u32 = 2;

const BACKTRACK_LIMIT: u32 = 1_000_000;
const RECURSION_LIMIT: u32 = 100_000;

struct Regex {
    code: Code,
    utf: bool,
    capture_count: usize,
    names: Rc<[(Vec<u8>, usize)]>,
    matcher: Matcher,
}

struct Code(*mut pcre2_code_8);

impl Drop for Code {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from `pcre2_compile_8`, was checked non-NULL, and is freed only here.
        unsafe { pcre2_code_free_8(self.0) }
    }
}

impl Code {
    fn info_u32(&self, what: u32) -> u32 {
        debug_assert!(matches!(
            what,
            PCRE2_INFO_CAPTURECOUNT | PCRE2_INFO_NAMECOUNT | PCRE2_INFO_NAMEENTRYSIZE
        ));
        let mut v: u32 = 0;
        // SAFETY: `self.0` is a live compiled pattern, and `what` is one of the items pcre2api documents as writing a
        // `uint32_t`, so PCRE2 writes exactly 4 bytes into `v`.
        unsafe { pcre2_pattern_info_8(self.0, what, (&raw mut v).cast()) };
        v
    }

    fn name_table(&self, count: u32, entry_size: u32) -> &[u8] {
        if count == 0 {
            return &[];
        }
        let mut table: *const u8 = std::ptr::null();
        // SAFETY: `self.0` is a live compiled pattern; PCRE2_INFO_NAMETABLE writes a `PCRE2_SPTR` into `table`.
        unsafe { pcre2_pattern_info_8(self.0, PCRE2_INFO_NAMETABLE, (&raw mut table).cast()) };
        // SAFETY: with `count > 0` the table is non-NULL and holds `count * entry_size` bytes owned by the compiled
        // pattern, which outlives the returned borrow of `self`.
        unsafe { std::slice::from_raw_parts(table, count as usize * entry_size as usize) }
    }
}

struct Matcher {
    md: *mut pcre2_match_data_8,
    mctx: *mut pcre2_match_context_8,
}

impl Drop for Matcher {
    fn drop(&mut self) {
        // SAFETY: both came from their `*_create` calls in `Matcher::new` and are freed only here; the `*_free`
        // functions return without doing anything on NULL (pcre2api), which happens when a create call failed.
        unsafe {
            pcre2_match_data_free_8(self.md);
            pcre2_match_context_free_8(self.mctx);
        }
    }
}

#[derive(Default)]
struct EndsHasher(u64);

impl Hasher for EndsHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, b: &[u8]) {
        let word = |at: usize| {
            let mut w = [0u8; 8];
            let end = b.len().min(at + 8);
            w[..end - at].copy_from_slice(&b[at..end]);
            u64::from_le_bytes(w)
        };
        let n = b.len();
        for x in [n as u64, word(0), word(n / 2), word(n.saturating_sub(8))] {
            self.0 = (self.0.rotate_left(5) ^ x).wrapping_mul(0x517c_c1b7_2722_0a95);
        }
    }
}

type Groups = Vec<Option<(usize, usize)>>;

type PatternMap<V> = HashMap<Vec<u8>, V, BuildHasherDefault<EndsHasher>>;

thread_local! {
    static CACHE: RefCell<PatternMap<Option<Rc<Regex>>>> = RefCell::default();
    static VARIANTS: RefCell<PatternMap<[Option<Rc<Regex>>; 2]>> = RefCell::default();
    /// Never freed, so it outlives every match context on this thread that points at it. NULL (out of memory) makes
    /// `pcre2_jit_stack_assign` fall back to PCRE2's 32 KB machine-stack default.
    // SAFETY: a plain allocation with the default (malloc) general context.
    static JIT_STACK: *mut pcre2_jit_stack_8 = unsafe { pcre2_jit_stack_create_8(32 * 1024, 192 * 1024, std::ptr::null_mut()) };
}

fn compile(pattern: &[u8]) -> Option<Rc<Regex>> {
    if let Some(r) = CACHE.with(|c| c.borrow().get(pattern).cloned()) {
        return r;
    }
    let r = compile_uncached(pattern).ok().map(Rc::new);
    CACHE.with(|c| c.borrow_mut().insert(pattern.to_vec(), r.clone()));
    r
}

fn compile_uncached(pattern: &[u8]) -> Result<Regex, String> {
    let (body, m) = pun_tokens::php_compat::preg_parse(pattern)?;
    let options = [
        (m.caseless, PCRE2_CASELESS),
        (m.multi_line, PCRE2_MULTILINE),
        (m.no_auto_capture, PCRE2_NO_AUTO_CAPTURE),
        (m.dotall, PCRE2_DOTALL),
        (m.extended, PCRE2_EXTENDED),
        (m.anchored, PCRE2_ANCHORED),
        (m.dollar_end_only, PCRE2_DOLLAR_ENDONLY),
        (m.ungreedy, PCRE2_UNGREEDY),
        (m.utf, PCRE2_UTF | PCRE2_UCP),
        (m.dup_names, PCRE2_DUPNAMES),
    ]
    .iter()
    .filter(|o| o.0)
    .fold(0u32, |all, o| all | o.1);
    let utf = m.utf;
    let mut err: c_int = 0;
    let mut erroff: usize = 0;
    // SAFETY: `body` is a live slice of `body.len()` bytes, which PCRE2 reads during the call and does not keep; the
    // out-parameters are live locals; a NULL compile context selects the defaults.
    let code = unsafe {
        pcre2_compile_8(
            body.as_ptr(),
            body.len(),
            options,
            &raw mut err,
            &raw mut erroff,
            std::ptr::null_mut(),
        )
    };
    if code.is_null() {
        let mut message = [0u8; 256];
        // SAFETY: `message` is a live buffer of `message.len()` bytes that PCRE2 writes the NUL-terminated text into.
        let n = unsafe { pcre2_get_error_message_8(err, message.as_mut_ptr(), message.len()) };
        let message = String::from_utf8_lossy(&message[..n as usize]);
        return Err(format!("Compilation failed: {message} at offset {erroff}"));
    }
    let code = Code(code);
    // SAFETY: `code` is a live compiled pattern. A failed JIT compile leaves the interpreter in charge, like PHP.
    unsafe { pcre2_jit_compile_8(code.0, PCRE2_JIT_COMPLETE) };
    let capture_count = code.info_u32(PCRE2_INFO_CAPTURECOUNT) as usize;
    let name_count = code.info_u32(PCRE2_INFO_NAMECOUNT);
    let entry_size = code.info_u32(PCRE2_INFO_NAMEENTRYSIZE);
    let names: Vec<(Vec<u8>, usize)> = code
        .name_table(name_count, entry_size)
        .chunks_exact(entry_size.max(1) as usize)
        .map(|entry| {
            let group = ((entry[0] as usize) << 8) | entry[1] as usize;
            let name = &entry[2..];
            let name = &name[..name.iter().position(|&b| b == 0).unwrap_or(name.len())];
            (name.to_vec(), group)
        })
        .collect();
    debug_assert!(names.is_sorted_by(|a, b| a.0 <= b.0));
    let matcher = Matcher::new(&code).ok_or_else(|| "Internal error".to_string())?;
    Ok(Regex {
        code,
        utf,
        capture_count,
        names: names.into(),
        matcher,
    })
}

impl Matcher {
    fn new(code: &Code) -> Option<Matcher> {
        // SAFETY: `code` is a live compiled pattern; a NULL general context selects malloc/free.
        let md = unsafe { pcre2_match_data_create_from_pattern_8(code.0, std::ptr::null_mut()) };
        // SAFETY: a NULL general context selects malloc/free.
        let mctx = unsafe { pcre2_match_context_create_8(std::ptr::null_mut()) };
        let m = Matcher { md, mctx };
        let m = (!m.md.is_null() && !m.mctx.is_null()).then_some(m)?;
        // SAFETY: `m.mctx` is a live match context (checked above). The JIT stack is this thread's and never freed
        // (see JIT_STACK), and `Regex` is not `Send`, so the context never outlives it or reaches another thread.
        unsafe {
            pcre2_set_match_limit_8(m.mctx, BACKTRACK_LIMIT);
            pcre2_set_depth_limit_8(m.mctx, RECURSION_LIMIT);
            JIT_STACK.with(|s| pcre2_jit_stack_assign_8(m.mctx, None, (*s).cast()));
        }
        Some(m)
    }
}

impl Regex {
    fn exec(&self, subject: &[u8], offset: usize, options: u32) -> Result<Option<Groups>, i32> {
        let m = &self.matcher;
        // SAFETY: the pattern, match data and context are live and belong together; `subject` is a live slice that
        // PCRE2 only reads during the call. PCRE2_NO_UTF_CHECK is sound because a `u` pattern only runs on valid
        // UTF-8 from a character boundary: `with_fallback` and `RawPreg::run` check the subject and start offset, and
        // `exec_all`/`split_impl` move past an empty match by a whole character (`unit_len`).
        let rc = unsafe {
            pcre2_match_8(
                self.code.0,
                subject.as_ptr(),
                subject.len(),
                offset,
                options | PCRE2_NO_UTF_CHECK,
                m.md,
                m.mctx,
            )
        };
        if rc == PCRE2_ERROR_NOMATCH {
            return Ok(None);
        }
        if rc < 0 {
            return Err(rc);
        }
        // SAFETY: match data created from this pattern has an ovector of `capture_count + 1` pairs, valid until the
        // next match on it; the slice is read before this function returns.
        let ov = unsafe {
            std::slice::from_raw_parts(
                pcre2_get_ovector_pointer_8(m.md),
                2 * (self.capture_count + 1),
            )
        };
        let groups = (0..=self.capture_count)
            .map(|i| {
                if i < rc as usize && ov[2 * i] != usize::MAX {
                    Some((ov[2 * i], ov[2 * i + 1]))
                } else {
                    None
                }
            })
            .collect();
        Ok(Some(groups))
    }

    fn unit_len(&self, subject: &[u8], at: usize) -> usize {
        if !self.utf {
            return 1;
        }
        match subject[at] {
            0xF0.. => 4,
            0xE0.. => 3,
            0xC0.. => 2,
            _ => 1,
        }
    }

    fn exec_all(&self, subject: &[u8], limit: Option<usize>) -> Result<Vec<Groups>, i32> {
        let mut out = Vec::new();
        let mut offset = 0;
        let mut options = 0;
        loop {
            if limit.is_some_and(|l| out.len() >= l) {
                break;
            }
            match self.exec(subject, offset, options)? {
                Some(g) => {
                    let (s, e) = g[0].unwrap();
                    out.push(g);
                    offset = e;
                    options = if s == e {
                        PCRE2_NOTEMPTY_ATSTART | PCRE2_ANCHORED
                    } else {
                        0
                    };
                }
                None => {
                    if options == 0 || offset >= subject.len() {
                        break;
                    }
                    offset += self.unit_len(subject, offset);
                    options = 0;
                }
            }
        }
        Ok(out)
    }

    fn captures_at<'s>(
        &self,
        subject: &'s [u8],
        offset: usize,
    ) -> Result<Option<Captures<'s>>, i32> {
        Ok(self
            .exec(subject, offset, 0)?
            .map(|g| Captures::new(subject, g, self)))
    }

    fn captures_all<'s>(&self, subject: &'s [u8]) -> Result<Vec<Captures<'s>>, i32> {
        Ok(self
            .exec_all(subject, None)?
            .into_iter()
            .map(|g| Captures::new(subject, g, self))
            .collect())
    }

    fn replace_with(
        &self,
        subject: &[u8],
        limit: Option<usize>,
        f: &dyn Fn(&Captures) -> Vec<u8>,
    ) -> Result<(Vec<u8>, usize), i32> {
        let all = self.exec_all(subject, limit)?;
        let mut out = Vec::with_capacity(subject.len());
        let mut last = 0;
        let count = all.len();
        for g in all {
            let (s, e) = g[0].unwrap();
            out.extend_from_slice(&subject[last..s]);
            out.extend(f(&Captures::new(subject, g, self)));
            last = e;
        }
        out.extend_from_slice(&subject[last..]);
        Ok((out, count))
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Captures<'s> {
    subject: &'s [u8],
    groups: Groups,
    len: usize,
    names: Rc<[(Vec<u8>, usize)]>,
}

impl<'s> Captures<'s> {
    fn new(subject: &'s [u8], groups: Groups, re: &Regex) -> Self {
        let len = groups
            .iter()
            .rposition(Option::is_some)
            .map_or(0, |i| i + 1);
        Captures {
            subject,
            groups,
            len,
            names: re.names.clone(),
        }
    }

    pub(crate) fn get(&self, i: usize) -> Option<&'s [u8]> {
        self.groups
            .get(i)
            .copied()
            .flatten()
            .map(|(s, e)| &self.subject[s..e])
    }

    pub(crate) fn str(&self, i: usize) -> &'s [u8] {
        self.get(i).unwrap_or(b"")
    }

    pub(crate) fn offset(&self, i: usize) -> Option<usize> {
        self.groups.get(i).copied().flatten().map(|(s, _)| s)
    }

    pub(crate) fn start(&self) -> usize {
        self.offset(0).expect("group 0 always matched")
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }

    fn group_by_name(&self, name: &[u8]) -> Option<usize> {
        let lo = self.names.partition_point(|(n, _)| n.as_slice() < name);
        self.names[lo..]
            .iter()
            .take_while(|(n, _)| n == name)
            .filter(|&&(_, g)| g < self.len)
            .map(|&(_, g)| g)
            .max()
    }

    pub(crate) fn name(&self, name: &str) -> Option<&'s [u8]> {
        self.get(self.group_by_name(name.as_bytes())?)
    }

    pub(crate) fn name_str(&self, name: &str) -> &'s [u8] {
        self.name(name).unwrap_or(b"")
    }

    pub(crate) fn name_exists(&self, name: &str) -> bool {
        self.group_by_name(name.as_bytes()).is_some()
    }

    pub(crate) fn name_offset(&self, name: &str) -> Option<usize> {
        self.offset(self.group_by_name(name.as_bytes())?)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Piece {
    pub text: Vec<u8>,
    pub offset: usize,
}

pub(crate) struct Preg;

fn has_line_break(s: &[u8]) -> bool {
    if s.iter().any(|b| matches!(b, b'\n' | b'\r' | 0x0b | 0x0c)) {
        return true;
    }
    if s.is_ascii() {
        return false;
    }
    match std::str::from_utf8(s) {
        Ok(t) => t.contains(['\u{85}', '\u{2028}', '\u{2029}']),
        Err(_) => s.contains(&0x85),
    }
}

fn add_utf8_modifier(pattern: &[u8]) -> Vec<u8> {
    let mut p = pattern.to_vec();
    p.push(b'u');
    p
}

fn remove_utf8_modifier(pattern: &[u8]) -> Vec<u8> {
    if pattern.is_empty() {
        return Vec::new();
    }
    let delimiter = pattern[0];
    let end = pattern.iter().rposition(|&b| b == delimiter).unwrap();
    let mut out = pattern[..end].to_vec();
    out.extend(pattern[end..].iter().filter(|&&b| b != b'u'));
    out
}

fn variants(pattern: &[u8], cached: bool) -> [Option<Rc<Regex>>; 2] {
    if !cached {
        return [
            compile_uncached(&add_utf8_modifier(pattern))
                .ok()
                .map(Rc::new),
            compile_uncached(&remove_utf8_modifier(pattern))
                .ok()
                .map(Rc::new),
        ];
    }
    VARIANTS
        .with(|v| v.borrow().get(pattern).cloned())
        .unwrap_or_else(|| {
            let v = [
                compile(&add_utf8_modifier(pattern)),
                compile(&remove_utf8_modifier(pattern)),
            ];
            VARIANTS.with(|m| m.borrow_mut().insert(pattern.to_vec(), v.clone()));
            v
        })
}

fn with_fallback<T>(
    pattern: &[u8],
    subject: &[u8],
    offset: usize,
    method: &str,
    cached: bool,
    f: impl Fn(&Regex) -> Result<T, i32>,
) -> T {
    let mut error = None;
    for re in variants(pattern, cached) {
        let Some(re) = re else {
            error = None;
            continue;
        };
        if re.utf
            && (std::str::from_utf8(subject).is_err()
                || (offset < subject.len() && (subject[offset] & 0xC0) == 0x80))
        {
            continue;
        }
        match f(&re) {
            Ok(r) => return r,
            Err(rc) => error = Some(rc),
        }
    }
    panic!("{}", preg_exception(error, method, pattern));
}

fn preg_last_error_msg(error: Option<i32>) -> &'static str {
    match error {
        Some(PCRE2_ERROR_MATCHLIMIT) => "Backtrack limit exhausted",
        Some(PCRE2_ERROR_JIT_STACKLIMIT) => "JIT stack limit exhausted",
        _ => "Internal error",
    }
}

fn preg_exception(error: Option<i32>, method: &str, pattern: &[u8]) -> String {
    match compile_uncached(pattern) {
        Ok(_) => format!(
            "Unknown error occurred when calling {method}: {}.",
            preg_last_error_msg(error)
        ),
        Err(warning) => format!(
            "{method}(): Invalid PCRE pattern \"{}\": (code: 1) {warning} (version: {})",
            String::from_utf8_lossy(pattern),
            pcre_version()
        ),
    }
}

fn pcre_version() -> String {
    let mut buf = [0u8; 64];
    // Returns the length including the terminating NUL.
    // SAFETY: `buf` is a live 64-byte buffer, larger than any PCRE2 version string, that PCRE2 writes into.
    let n = unsafe { pcre2_config_8(PCRE2_CONFIG_VERSION, buf.as_mut_ptr().cast::<c_void>()) };
    String::from_utf8_lossy(&buf[..n as usize - 1]).into_owned()
}

impl Preg {
    pub(crate) fn r#match(pattern: impl AsRef<[u8]>, subject: impl AsRef<[u8]>) -> bool {
        if pattern.as_ref() == br"/\R/" {
            return has_line_break(subject.as_ref());
        }
        Self::match_groups(pattern, subject.as_ref()).is_some()
    }

    pub(crate) fn match_groups(pattern: impl AsRef<[u8]>, subject: &[u8]) -> Option<Captures<'_>> {
        Self::match_at(pattern, subject, 0)
    }

    pub(crate) fn match_groups_uncached(
        pattern: impl AsRef<[u8]>,
        subject: &[u8],
    ) -> Option<Captures<'_>> {
        with_fallback(
            pattern.as_ref(),
            subject,
            0,
            "PhpCsFixer\\Preg::match",
            false,
            |re| re.captures_at(subject, 0),
        )
    }

    pub(crate) fn match_at(
        pattern: impl AsRef<[u8]>,
        subject: &[u8],
        offset: usize,
    ) -> Option<Captures<'_>> {
        with_fallback(
            pattern.as_ref(),
            subject,
            offset,
            "PhpCsFixer\\Preg::match",
            true,
            |re| re.captures_at(subject, offset),
        )
    }

    pub(crate) fn match_all(pattern: impl AsRef<[u8]>, subject: &[u8]) -> Vec<Captures<'_>> {
        with_fallback(
            pattern.as_ref(),
            subject,
            0,
            "PhpCsFixer\\Preg::matchAll",
            true,
            |re| re.captures_all(subject),
        )
    }

    pub(crate) fn replace(
        pattern: impl AsRef<[u8]>,
        replacement: impl AsRef<[u8]>,
        subject: impl AsRef<[u8]>,
    ) -> Vec<u8> {
        Self::replace_limit(pattern, replacement, subject, -1).0
    }

    pub(crate) fn replace_limit(
        pattern: impl AsRef<[u8]>,
        replacement: impl AsRef<[u8]>,
        subject: impl AsRef<[u8]>,
        limit: isize,
    ) -> (Vec<u8>, usize) {
        Self::replace_expanded(
            pattern.as_ref(),
            replacement.as_ref(),
            subject.as_ref(),
            limit,
            true,
        )
    }

    pub(crate) fn replace_limit_uncached(
        pattern: impl AsRef<[u8]>,
        replacement: impl AsRef<[u8]>,
        subject: impl AsRef<[u8]>,
        limit: isize,
    ) -> (Vec<u8>, usize) {
        Self::replace_expanded(
            pattern.as_ref(),
            replacement.as_ref(),
            subject.as_ref(),
            limit,
            false,
        )
    }

    fn replace_expanded(
        pattern: &[u8],
        replacement: &[u8],
        subject: &[u8],
        limit: isize,
        cached: bool,
    ) -> (Vec<u8>, usize) {
        Self::replace_impl(
            pattern,
            subject,
            limit,
            "PhpCsFixer\\Preg::replace",
            cached,
            &|c: &Captures| expand_replacement(replacement, c),
        )
    }

    pub(crate) fn replace_callback(
        pattern: impl AsRef<[u8]>,
        callback: impl Fn(&Captures) -> Vec<u8>,
        subject: impl AsRef<[u8]>,
    ) -> Vec<u8> {
        Self::replace_callback_limit(pattern, callback, subject, -1).0
    }

    pub(crate) fn replace_callback_limit(
        pattern: impl AsRef<[u8]>,
        callback: impl Fn(&Captures) -> Vec<u8>,
        subject: impl AsRef<[u8]>,
        limit: isize,
    ) -> (Vec<u8>, usize) {
        Self::replace_impl(
            pattern.as_ref(),
            subject.as_ref(),
            limit,
            "PhpCsFixer\\Preg::replaceCallback",
            true,
            &callback,
        )
    }

    fn replace_impl(
        pattern: &[u8],
        subject: &[u8],
        limit: isize,
        method: &str,
        cached: bool,
        f: &dyn Fn(&Captures) -> Vec<u8>,
    ) -> (Vec<u8>, usize) {
        let limit = (limit >= 0).then_some(limit as usize);
        with_fallback(pattern, subject, 0, method, cached, |re| {
            re.replace_with(subject, limit, f)
        })
    }

    pub(crate) fn split(
        pattern: impl AsRef<[u8]>,
        subject: impl AsRef<[u8]>,
        limit: isize,
        flags: u32,
    ) -> Vec<Piece> {
        let (pattern, subject) = (pattern.as_ref(), subject.as_ref());
        with_fallback(pattern, subject, 0, "PhpCsFixer\\Preg::split", true, |re| {
            split_impl(re, subject, limit, flags)
        })
    }

    pub(crate) fn split_str(pattern: impl AsRef<[u8]>, subject: impl AsRef<[u8]>) -> Vec<Vec<u8>> {
        Self::split(pattern, subject, -1, 0)
            .into_iter()
            .map(|p| p.text)
            .collect()
    }
}

pub(crate) use pun_tokens::php_compat::preg_quote;

fn split_impl(re: &Regex, subject: &[u8], limit: isize, flags: u32) -> Result<Vec<Piece>, i32> {
    let no_empty = flags & PREG_SPLIT_NO_EMPTY != 0;
    let delim_capture = flags & PREG_SPLIT_DELIM_CAPTURE != 0;
    let mut limit_val = if limit == 0 { -1 } else { limit };
    let mut out = Vec::new();
    let push = |out: &mut Vec<Piece>, s: usize, e: usize| {
        out.push(Piece {
            text: subject[s..e].to_vec(),
            offset: s,
        });
    };
    let mut offset = 0;
    let mut last_match_offset = 0;
    let mut options = 0;
    if limit_val == -1 || limit_val > 1 {
        loop {
            match re.exec(subject, offset, options)? {
                Some(g) => {
                    let (s, e) = g[0].unwrap();
                    if !no_empty || s != last_match_offset {
                        push(&mut out, last_match_offset, s);
                        if limit_val != -1 {
                            limit_val -= 1;
                        }
                    }
                    if delim_capture {
                        let count = g.iter().rposition(Option::is_some).map_or(0, |i| i + 1);
                        for grp in g.iter().take(count).skip(1) {
                            let (gs, ge) = grp.unwrap_or((0, 0));
                            if !no_empty || gs != ge {
                                push(&mut out, gs, ge);
                            }
                        }
                    }
                    offset = e;
                    last_match_offset = e;
                    if s == e {
                        if limit_val != -1 && limit_val <= 1 {
                            break;
                        }
                        options = PCRE2_NOTEMPTY_ATSTART | PCRE2_ANCHORED;
                    } else {
                        options = 0;
                    }
                }
                None => {
                    if options != 0 && offset < subject.len() {
                        offset += re.unit_len(subject, offset);
                        options = 0;
                    } else {
                        break;
                    }
                }
            }
            if !(limit_val == -1 || limit_val > 1) {
                break;
            }
        }
    }
    let start = last_match_offset;
    if !no_empty || start < subject.len() {
        push(&mut out, start, subject.len());
    }
    Ok(out)
}

fn expand_replacement(replacement: &[u8], c: &Captures) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut walk_last = 0u8;
    while i < replacement.len() {
        let ch = replacement[i];
        if (ch == b'\\' || ch == b'$') && walk_last == b'\\' {
            *out.last_mut().unwrap() = ch;
            walk_last = 0;
            i += 1;
            continue;
        }
        if (ch == b'\\' || ch == b'$') && i + 1 < replacement.len() {
            let mut w = i;
            let in_brace = ch == b'$' && replacement[w + 1] == b'{';
            if in_brace {
                w += 1;
            }
            w += 1;
            if w < replacement.len() && replacement[w].is_ascii_digit() {
                let mut backref = (replacement[w] - b'0') as usize;
                w += 1;
                if w < replacement.len() && replacement[w].is_ascii_digit() {
                    backref = backref * 10 + (replacement[w] - b'0') as usize;
                    w += 1;
                }
                let ok = if in_brace {
                    if w < replacement.len() && replacement[w] == b'}' {
                        w += 1;
                        true
                    } else {
                        false
                    }
                } else {
                    true
                };
                if ok {
                    out.extend_from_slice(c.str(backref));
                    i = w;
                    walk_last = 0;
                    continue;
                }
            }
        }
        out.push(ch);
        walk_last = ch;
        i += 1;
    }
    out
}

/// PHP's bare `preg_*` functions as Pint's Blade code calls them. A PCRE failure makes PHP return `false` with
/// empty matches, which that code reads as no match, so the matching functions return no match on error too.
pub(crate) struct RawPreg;

impl RawPreg {
    fn try_run<T>(
        pattern: &[u8],
        subject: &[u8],
        f: impl Fn(&Regex) -> Result<T, i32>,
    ) -> Result<T, &'static str> {
        let re = compile(pattern).ok_or(preg_last_error_msg(None))?;
        if re.utf && std::str::from_utf8(subject).is_err() {
            return Err("Malformed UTF-8 characters, possibly incorrectly encoded");
        }
        f(&re).map_err(|rc| preg_last_error_msg(Some(rc)))
    }

    fn run<T>(pattern: &[u8], subject: &[u8], f: impl Fn(&Regex) -> Result<T, i32>) -> Option<T> {
        Self::try_run(pattern, subject, f).ok()
    }

    pub(crate) fn match_groups(pattern: impl AsRef<[u8]>, subject: &[u8]) -> Option<Captures<'_>> {
        Self::run(pattern.as_ref(), subject, |re| re.captures_at(subject, 0)).flatten()
    }

    pub(crate) fn r#match(pattern: impl AsRef<[u8]>, subject: impl AsRef<[u8]>) -> bool {
        Self::match_groups(pattern, subject.as_ref()).is_some()
    }

    pub(crate) fn match_all(pattern: impl AsRef<[u8]>, subject: &[u8]) -> Vec<Captures<'_>> {
        Self::run(pattern.as_ref(), subject, |re| re.captures_all(subject)).unwrap_or_default()
    }

    pub(crate) fn replace_callback(
        pattern: impl AsRef<[u8]>,
        callback: impl Fn(&Captures) -> Vec<u8>,
        subject: impl AsRef<[u8]>,
    ) -> Result<Vec<u8>, &'static str> {
        let subject = subject.as_ref();
        Self::try_run(pattern.as_ref(), subject, |re| {
            re.replace_with(subject, None, &callback)
        })
        .map(|(out, _)| out)
    }

    pub(crate) fn replace(
        pattern: impl AsRef<[u8]>,
        replacement: impl AsRef<[u8]>,
        subject: impl AsRef<[u8]>,
    ) -> Result<Vec<u8>, &'static str> {
        let replacement = replacement.as_ref();
        Self::replace_callback(pattern, |c| expand_replacement(replacement, c), subject)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn line_break_fast_path() {
        let cases: &[(&[u8], bool)] = &[
            (b"", false),
            (b"  ", false),
            (b"a\nb", true),
            (b"\r", true),
            (b"\x0b", true),
            (b"\x0c", true),
            ("é".as_bytes(), false),
            (b"\xc2\x85", true),
            (b"\x85", true),
            (b"a\xe2\x80\xa8", true),
            (b"\xe2\x80\xa9", true),
            (b"\xff\x85", true),
            (b"\xff", false),
        ];
        for &(s, want) in cases {
            assert_eq!(super::Preg::r#match(r"/\R/", s), want, "{s:?}");
        }
    }

    use super::*;

    #[test]
    fn php_semantics() {
        let s = |v: Vec<Piece>| {
            v.into_iter()
                .map(|p| String::from_utf8(p.text).unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(s(Preg::split("//", "abc", -1, 0)), ["", "a", "b", "c", ""]);
        assert_eq!(
            s(Preg::split("/(,)/", "a,b,,c", -1, PREG_SPLIT_DELIM_CAPTURE)),
            ["a", ",", "b", ",", "", ",", "c"]
        );
        assert_eq!(
            s(Preg::split("/,/", "a,b,,c", -1, PREG_SPLIT_NO_EMPTY)),
            ["a", "b", "c"]
        );
        assert_eq!(s(Preg::split("/,/", "a,b,c", 2, 0)), ["a", "b,c"]);
        assert_eq!(s(Preg::split("/x*/", "axb", -1, 0)), ["", "a", "", "b", ""]);
        assert_eq!(Preg::replace("/x*/", "-", "abc"), b"-a-b-c-");
        assert_eq!(Preg::replace("/x*/", "-", "axxb"), b"-a--b-");
        assert_eq!(
            Preg::replace("/(a)(b)?/", "[$2|\\1|${1}0]", "ac"),
            b"[|a|a0]c"
        );
        assert_eq!(Preg::replace("/./", "x", "é"), b"x");
        assert_eq!(Preg::replace("/./", "x", b"\xe9"), b"x");
        assert_eq!(Preg::replace("/./", "x", b"\xff\xfe"), b"xx");
        assert_eq!(
            Preg::replace_limit("/a/", "b", "aaa", 2),
            (b"bba".to_vec(), 2)
        );
        let m = Preg::match_groups("/(a)(b)?(c)?/", b"a").unwrap();
        assert_eq!(m.len(), 2);
        let m = Preg::match_groups("/(a)(x)?(c)/", b"ac").unwrap();
        assert_eq!((m.len(), m.str(2), m.offset(2)), (4, &b""[..], None));
        let m = Preg::match_groups("/(?<n>b)/", b"ab").unwrap();
        assert_eq!(
            (m.name("n"), m.name_offset("n")),
            (Some(&b"b"[..]), Some(1))
        );
        assert!(Preg::r#match(
            "{[/\\\\](stub|fixture)s?[/\\\\]}i",
            "a/Fixtures/b"
        ));
        assert!(
            Preg::r#match("/a$/D", "a")
                && !Preg::r#match("/a$/D", "a\n")
                && Preg::r#match("/a$/", "a\n")
        );
        assert_eq!(Preg::match_all("/\\d/", b"a1b2").len(), 2);
        assert_eq!(Preg::replace("/^/m", ">", "a\nb"), b">a\n>b");
        assert_eq!(Preg::replace("/\\R/", "|", "a\r\nb\nc"), b"a|b|c");
        assert_eq!(preg_quote(b"a.b/c\0", Some(b'/')), b"a\\.b\\/c\\000");
    }

    #[test]
    fn php_semantics_delimiters_modifiers_errors() {
        for (p, s, m0) in [
            ("(a)", "a", "a"),
            ("[A]i", "xa", "a"),
            ("{a{2}}", "aa", "aa"),
            ("{a\\}b}", "a}b", "a}b"),
            ("<a>", "a", "a"),
            ("/a\\/b/", "a/b", "a/b"),
            ("/a b # c/x", "ab", "ab"),
            ("/a/S", "a", "a"),
            ("/a/X", "a", "a"),
            ("/a+?/U", "aa", "aa"),
            ("/a/ \n", "a", "a"),
        ] {
            assert_eq!(
                Preg::match_groups(p, s.as_bytes()).map(|m| m.str(0).to_vec()),
                Some(m0.as_bytes().to_vec()),
                "{p}"
            );
        }
        assert!(!Preg::r#match("/b/A", "ab"));
        let m = Preg::match_groups("/(a)(?<n>b)?/n", b"ab").unwrap();
        assert_eq!(
            (m.len(), m.str(1), m.name("n")),
            (2, &b"b"[..], Some(&b"b"[..]))
        );
        let m = Preg::match_groups("/(?<x>a)|(?<x>b)/J", b"b").unwrap();
        assert_eq!(
            (m.str(1), m.str(2), m.name_str("x")),
            (&b""[..], &b"b"[..], &b"b"[..])
        );
        let m = Preg::match_groups("/(?<x>a)|(?<x>b)/J", b"a").unwrap();
        assert_eq!(
            (
                m.len(),
                m.name_str("x"),
                m.name_exists("x"),
                m.name_exists("y")
            ),
            (2, &b"a"[..], true, false)
        );
        let message = |f: &dyn Fn()| -> String {
            let e = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_err();
            e.downcast_ref::<String>().unwrap().clone()
        };
        for (p, warning) in [
            ("", "Empty regular expression"),
            ("  ", "Empty regular expression"),
            (
                "abc",
                "Delimiter must not be alphanumeric, backslash, or NUL byte",
            ),
            (
                "\\a\\",
                "Delimiter must not be alphanumeric, backslash, or NUL byte",
            ),
            ("/abc", "No ending delimiter '/' found"),
            ("{abc", "No ending matching delimiter '}' found"),
            ("(abc", "No ending matching delimiter ')' found"),
            ("/a/e", "Unknown modifier 'e'"),
            ("/a/\0", "NUL byte is not a valid modifier"),
            (
                "/(/",
                "Compilation failed: missing closing parenthesis at offset 1",
            ),
            (
                "/(?<n>a)(?<n>b)/",
                "Compilation failed: two named subpatterns have the same name (PCRE2_DUPNAMES not set) at offset 12",
            ),
        ] {
            let m = message(&|| {
                Preg::r#match(p, "a");
            });
            let prefix = format!(
                "PhpCsFixer\\Preg::match(): Invalid PCRE pattern \"{p}\": (code: 1) {warning} (version: 10."
            );
            assert!(m.starts_with(&prefix) && m.ends_with(')'), "{p:?}: {m}");
        }
        let unknown = |method: &str, error: &str| {
            format!("Unknown error occurred when calling PhpCsFixer\\Preg::{method}: {error}.")
        };
        assert_eq!(
            message(&|| {
                Preg::match_at("/a/", b"abc", 5);
            }),
            unknown("match", "Internal error")
        );
        let backtrack = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa!";
        assert_eq!(
            message(&|| {
                Preg::r#match("/(a*)*[bc\\x{100}]/u", backtrack);
            }),
            unknown("match", "Internal error")
        );
        let m = message(&|| {
            Preg::r#match("/(a*)*[bc\\x{100}]/", backtrack);
        });
        assert!(m.starts_with("PhpCsFixer\\Preg::match(): Invalid PCRE pattern \"/(a*)*[bc\\x{100}]/\": (code: 1) Compilation failed: character code point value in \\x{} or \\o{} is too large at offset 14 (version: 10."), "{m}");
        assert_eq!(
            message(&|| {
                Preg::r#match("/(.)*/", "x".repeat(200_000));
            }),
            unknown("match", "JIT stack limit exhausted")
        );
        assert_eq!(
            message(&|| {
                Preg::r#match("/(a*)*[bc]/", backtrack);
            }),
            unknown("match", "Backtrack limit exhausted")
        );
        assert_eq!(
            message(&|| {
                Preg::match_all("/(a*)*[bc]/", backtrack.as_bytes());
            }),
            unknown("matchAll", "Backtrack limit exhausted")
        );
        assert_eq!(
            message(&|| {
                Preg::replace("/(a*)*[bc]/", "x", backtrack);
            }),
            unknown("replace", "Backtrack limit exhausted")
        );
        assert_eq!(
            message(&|| {
                Preg::replace_callback("/(a*)*[bc]/", |_| b"x".to_vec(), backtrack);
            }),
            unknown("replaceCallback", "Backtrack limit exhausted")
        );
        assert_eq!(
            message(&|| {
                Preg::split("/(a*)*[bc]/", backtrack, -1, 0);
            }),
            unknown("split", "Backtrack limit exhausted")
        );
        let s = |v: Vec<Piece>| {
            v.into_iter()
                .map(|p| String::from_utf8(p.text).unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            s(Preg::split("//", "é日😀", -1, PREG_SPLIT_NO_EMPTY)),
            ["é", "日", "😀"]
        );
        assert_eq!(Preg::replace("/x*/", "-", b"\xff\xfe"), b"-\xff-\xfe-");
        assert_eq!(s(Preg::split("/x*/", "abc", 2, 0)), ["", "abc"]);
        assert_eq!(s(Preg::split("/x*/", "abc", 1, 0)), ["abc"]);
        let pieces = Preg::split("/(,)/", "a,b", -1, PREG_SPLIT_DELIM_CAPTURE);
        assert_eq!(
            pieces
                .iter()
                .map(|p| (p.text.as_slice(), p.offset))
                .collect::<Vec<_>>(),
            [(&b"a"[..], 0), (b",", 1), (b"b", 2)]
        );
        assert_eq!(
            Preg::split_str("/,/", "a,b"),
            [b"a".to_vec(), b"b".to_vec()]
        );
        assert_eq!(
            Preg::replace(
                "/(a)(b)(c)(d)(e)(f)(g)(h)(i)(j)(k)/",
                r"[$11|\11|${11}|${1|$1x|\\|$|\",
                "abcdefghijk"
            ),
            br"[k|k|k|${1|ax|\|$|\"
        );
        assert_eq!(Preg::replace("/(a)/", r"${1}2${99}$99\99", "a"), b"a2");
        assert_eq!(
            Preg::replace("/(a)/", r"\\1|\$1|\\\1|\a|$$1", "a"),
            br"\1|$1|\a|\a|$a"
        );
    }

    #[test]
    fn regex_is_not_send() {
        trait AmbiguousIfSend<A> {
            fn check() {}
        }
        impl<T: ?Sized> AmbiguousIfSend<()> for T {}
        impl<T: ?Sized + Send> AmbiguousIfSend<u8> for T {}
        <Regex as AmbiguousIfSend<_>>::check();
    }

    #[test]
    fn uncached_patterns_are_not_kept() {
        let sizes = || {
            (
                CACHE.with(|c| c.borrow().len()),
                VARIANTS.with(|v| v.borrow().len()),
            )
        };
        let before = sizes();
        let m = Preg::match_groups_uncached(r"/@param\s+(?<v>\$\w+)/", b" * @param $x").unwrap();
        assert_eq!(m.name_str("v"), b"$x");
        assert_eq!(
            Preg::replace_limit_uncached("/a/", "b", "aaa", 2),
            (b"bba".to_vec(), 2)
        );
        assert_eq!(sizes(), before);
        Preg::match_groups(r"/@param\s+(?<v>\$\w+)/", b" * @param $x");
        assert_eq!(sizes(), (before.0 + 2, before.1 + 1));
    }

    #[test]
    fn raw_preg_php_values() {
        let m = RawPreg::match_groups("/(a)(b)?/", b"xa").unwrap();
        assert_eq!((m.len(), m.str(1)), (2, &b"a"[..]));
        let all = RawPreg::match_all("/a(\\d)/", b"a1 a2");
        assert_eq!(
            (all.len(), all[1].str(1), all[1].offset(1)),
            (2, &b"2"[..], Some(4))
        );
        assert!(!RawPreg::r#match("/a/u", b"\xff"));
        assert!(RawPreg::r#match("/a/", "ba"));
        assert_eq!(RawPreg::replace("/(/", "", "a"), Err("Internal error"));
        assert_eq!(
            RawPreg::replace("/a/u", "", b"\xff"),
            Err("Malformed UTF-8 characters, possibly incorrectly encoded")
        );
        assert_eq!(
            RawPreg::replace("/(a*)*[bc]/", "", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa!"),
            Err("Backtrack limit exhausted")
        );
        assert_eq!(
            RawPreg::replace("/(a)/", "[$1]", "aba"),
            Ok(b"[a]b[a]".to_vec())
        );
        assert_eq!(
            RawPreg::replace_callback("/b/", |c| c.str(0).to_ascii_uppercase(), "abcb"),
            Ok(b"aBcB".to_vec())
        );
    }
}
