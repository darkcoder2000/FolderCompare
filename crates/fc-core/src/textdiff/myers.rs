//! Myers' O(ND) difference algorithm (linear-space, middle-snake variant) on integer sequences.

use std::collections::HashSet;

/// Above this amount of work per middle-snake search the split falls back to the midpoint. The result is still a
/// valid (just not minimal) edit script.
const MAX_WORK_PER_SNAKE: i64 = 20_000_000;

/// Marks every element of `a` and `b` that is not part of the common subsequence.
pub fn compute(a: &[u32], b: &[u32]) -> (Vec<bool>, Vec<bool>) {
    let mut mod_a = vec![false; a.len()];
    let mut mod_b = vec![false; b.len()];

    // Elements that never occur on the other side can never match. Marking them up front keeps D small for files
    // that have little in common, which is where Myers gets slow.
    let in_b: HashSet<u32> = b.iter().copied().collect();
    let in_a: HashSet<u32> = a.iter().copied().collect();
    let mut idx_a = Vec::with_capacity(a.len());
    let mut idx_b = Vec::with_capacity(b.len());
    for (i, x) in a.iter().enumerate() {
        if in_b.contains(x) {
            idx_a.push(i);
        } else {
            mod_a[i] = true;
        }
    }
    for (j, y) in b.iter().enumerate() {
        if in_a.contains(y) {
            idx_b.push(j);
        } else {
            mod_b[j] = true;
        }
    }

    let ca: Vec<u32> = idx_a.iter().map(|&i| a[i]).collect();
    let cb: Vec<u32> = idx_b.iter().map(|&j| b[j]).collect();
    let mut runner = Runner::new(&ca, &cb);
    runner.lcs(0, ca.len(), 0, cb.len());

    for (i, &m) in runner.mod_a.iter().enumerate() {
        if m {
            mod_a[idx_a[i]] = true;
        }
    }
    for (j, &m) in runner.mod_b.iter().enumerate() {
        if m {
            mod_b[idx_b[j]] = true;
        }
    }
    (mod_a, mod_b)
}

struct Runner<'a> {
    a: &'a [u32],
    b: &'a [u32],
    mod_a: Vec<bool>,
    mod_b: Vec<bool>,
    down: Vec<isize>,
    up: Vec<isize>,
    max: isize,
}

impl<'a> Runner<'a> {
    fn new(a: &'a [u32], b: &'a [u32]) -> Self {
        let max = (a.len() + b.len() + 1) as isize;
        Self {
            a,
            b,
            mod_a: vec![false; a.len()],
            mod_b: vec![false; b.len()],
            down: vec![0; (2 * max + 2) as usize],
            up: vec![0; (2 * max + 2) as usize],
            max,
        }
    }

    /// Marks the differences in `a[lower_a..upper_a]` vs `b[lower_b..upper_b]`. The recursion of the original is
    /// an explicit stack here, so deep splits can't overflow the thread stack.
    fn lcs(&mut self, lower_a: usize, upper_a: usize, lower_b: usize, upper_b: usize) {
        let mut stack = vec![(lower_a, upper_a, lower_b, upper_b)];
        while let Some((mut la, mut ua, mut lb, mut ub)) = stack.pop() {
            while la < ua && lb < ub && self.a[la] == self.b[lb] {
                la += 1;
                lb += 1;
            }
            while la < ua && lb < ub && self.a[ua - 1] == self.b[ub - 1] {
                ua -= 1;
                ub -= 1;
            }
            if la == ua {
                self.mod_b[lb..ub].fill(true);
            } else if lb == ub {
                self.mod_a[la..ua].fill(true);
            } else {
                let (x, y) = self.middle_snake(la as isize, ua as isize, lb as isize, ub as isize);
                let (x, y) = (x as usize, y as usize);
                // Same order as the recursive version: the first half is processed first.
                stack.push((x, ua, y, ub));
                stack.push((la, x, lb, y));
            }
        }
    }

    fn middle_snake(
        &mut self,
        lower_a: isize,
        upper_a: isize,
        lower_b: isize,
        upper_b: isize,
    ) -> (isize, isize) {
        let down_k = lower_a - lower_b;
        let up_k = upper_a - upper_b;
        let delta = (upper_a - lower_a) - (upper_b - lower_b);
        let odd_delta = delta & 1 != 0;
        let down_offset = self.max - down_k;
        let up_offset = self.max - up_k;
        let max_d = (upper_a - lower_a + upper_b - lower_b) / 2 + 1;
        let size = (upper_a - lower_a + upper_b - lower_b) as i64;
        let (a, b) = (self.a, self.b);
        let at = |i: isize| a[i as usize];
        let bt = |j: isize| b[j as usize];
        macro_rules! down {
            ($i:expr) => {
                self.down[($i) as usize]
            };
        }
        macro_rules! up {
            ($i:expr) => {
                self.up[($i) as usize]
            };
        }

        down!(down_offset + down_k + 1) = lower_a;
        up!(up_offset + up_k - 1) = upper_a;

        for d in 0..=max_d {
            if d as i64 * size > MAX_WORK_PER_SNAKE {
                return ((lower_a + upper_a) / 2, (lower_b + upper_b) / 2);
            }

            let mut k = down_k - d;
            while k <= down_k + d {
                let mut x;
                if k == down_k - d {
                    x = down!(down_offset + k + 1);
                } else {
                    x = down!(down_offset + k - 1) + 1;
                    if k < down_k + d && down!(down_offset + k + 1) >= x {
                        x = down!(down_offset + k + 1);
                    }
                }
                let mut y = x - k;
                while x < upper_a && y < upper_b && at(x) == bt(y) {
                    x += 1;
                    y += 1;
                }
                down!(down_offset + k) = x;

                if odd_delta
                    && up_k - d < k
                    && k < up_k + d
                    && up!(up_offset + k) <= down!(down_offset + k)
                {
                    let x = down!(down_offset + k);
                    return (x, x - k);
                }
                k += 2;
            }

            let mut k = up_k - d;
            while k <= up_k + d {
                let mut x;
                if k == up_k + d {
                    x = up!(up_offset + k - 1);
                } else {
                    x = up!(up_offset + k + 1) - 1;
                    if k > up_k - d && up!(up_offset + k - 1) < x {
                        x = up!(up_offset + k - 1);
                    }
                }
                let mut y = x - k;
                while x > lower_a && y > lower_b && at(x - 1) == bt(y - 1) {
                    x -= 1;
                    y -= 1;
                }
                up!(up_offset + k) = x;

                if !odd_delta
                    && down_k - d <= k
                    && k <= down_k + d
                    && up!(up_offset + k) <= down!(down_offset + k)
                {
                    let x = down!(down_offset + k);
                    return (x, x - k);
                }
                k += 2;
            }
        }

        unreachable!("middle snake not found")
    }
}
