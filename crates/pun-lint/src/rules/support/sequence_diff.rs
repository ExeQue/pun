#[derive(Clone, Copy)]
pub(crate) enum Elem {
    Keep(usize, usize),
    Remove(usize),
    Add(usize),
    Replace(usize, usize),
}

pub(crate) fn diff(n: usize, new: &[usize]) -> Vec<Elem> {
    let m = new.len();
    let max = (n + m) as isize;
    let at = |k: isize| (k + max + 1) as usize;
    let mut v = vec![0usize; 2 * max as usize + 3];
    let mut trace: Vec<Vec<usize>> = Vec::new();
    let (mut x, mut y) = (0, 0);
    'outer: for d in 0..=max {
        trace.push(v.clone());
        let mut k = -d;
        while k <= d {
            x = if k == -d || (k != d && v[at(k - 1)] < v[at(k + 1)]) {
                v[at(k + 1)]
            } else {
                v[at(k - 1)] + 1
            };
            y = (x as isize - k) as usize;
            while x < n && y < m && x == new[y] {
                x += 1;
                y += 1;
            }
            v[at(k)] = x;
            if x >= n && y >= m {
                break 'outer;
            }
            k += 2;
        }
    }
    let mut out = Vec::new();
    for d in (0..trace.len() as isize).rev() {
        let v = &trace[d as usize];
        let k = x as isize - y as isize;
        let prev_k = if k == -d || (k != d && v[at(k - 1)] < v[at(k + 1)]) {
            k + 1
        } else {
            k - 1
        };
        let prev_x = v[at(prev_k)];
        let prev_y = prev_x as isize - prev_k;
        while x > prev_x && y as isize > prev_y {
            out.push(Elem::Keep(x - 1, y - 1));
            x -= 1;
            y -= 1;
        }
        if d == 0 {
            break;
        }
        while x > prev_x {
            out.push(Elem::Remove(x - 1));
            x -= 1;
        }
        while y as isize > prev_y {
            out.push(Elem::Add(y - 1));
            y -= 1;
        }
    }
    out.reverse();
    out
}

pub(crate) fn coalesce_replacements(diff: Vec<Elem>) -> Vec<Elem> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < diff.len() {
        if !matches!(diff[i], Elem::Remove(_)) {
            out.push(diff[i]);
            i += 1;
            continue;
        }
        let mut j = i;
        while j < diff.len() && matches!(diff[j], Elem::Remove(_)) {
            j += 1;
        }
        let mut k = j;
        while k < diff.len() && matches!(diff[k], Elem::Add(_)) {
            k += 1;
        }
        if j - i == k - j {
            for n in 0..j - i {
                if let (Elem::Remove(old), Elem::Add(new)) = (diff[i + n], diff[j + n]) {
                    out.push(Elem::Replace(old, new));
                }
            }
        } else {
            out.extend_from_slice(&diff[i..k]);
        }
        i = k;
    }
    out
}
