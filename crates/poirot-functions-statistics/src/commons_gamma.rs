/* ====================================================================
   Licensed to the Apache Software Foundation (ASF) under one or more
   contributor license agreements.  See the NOTICE file distributed with
   this work for additional information regarding copyright ownership.
   The ASF licenses this file to You under the Apache License, Version 2.0
   (the "License"); you may not use this file except in compliance with
   the License.  You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
==================================================================== */

//! Apache Commons Math 3.6.1 Gamma algorithms, Apache-2.0.
//! Coefficients and operation order follow the Java implementation; elementary
//! functions use Rust platform math, so final bits can differ from Java FastMath.
use crate::iteration::{KernelFailure, KernelResult, SourceConvergenceFailure, step};
// Exact authenticated Commons coefficient encoding; preserves the original decimal bits.
const COMMONS_EULER_GAMMA: f64 = f64::from_bits(0x3fe2_788c_fc6f_b619);
const NEG_B: &[f64] = &[
    1.9575583661463974E-10,
    -6.077618957228252E-8,
    9.926418406727737E-7,
    -6.4304548177935305E-6,
    -8.514194324403149E-6,
    4.939449793824468E-4,
    0.026620534842894922,
    0.203610414066807,
    1.0,
];
const P: &[f64] = &[
    4.343529937408594E-15,
    -1.2494415722763663E-13,
    1.5728330277104463E-12,
    4.686843322948848E-11,
    6.820161668496171E-10,
    6.8716741130671986E-9,
    6.116095104481416E-9,
];
const Q: &[f64] = &[
    2.6923694661863613E-4,
    0.004956830093825887,
    0.054642130860422966,
    0.3056961078365221,
    1.0,
];
const C: &[f64] = &[
    1.133027231981696E-6,
    -1.2504934821426706E-6,
    -2.013485478078824E-5,
    1.280502823881162E-4,
    -2.1524167411495098E-4,
    -0.0011651675918590652,
    0.0072189432466631,
    -0.009621971527876973,
    -0.04219773455554433,
    0.16653861138229148,
    -0.04200263503409524,
    -0.6558780715202539,
    -0.42278433509846713,
];
const LANCZOS: &[f64] = &[
    0.9999999999999971,
    57.15623566586292,
    -59.59796035547549,
    14.136097974741746,
    -0.4919138160976202,
    3.399464998481189E-5,
    4.652362892704858E-5,
    -9.837447530487956E-5,
    1.580887032249125E-4,
    -2.1026444172410488E-4,
    2.1743961811521265E-4,
    -1.643181065367639E-4,
    8.441822398385275E-5,
    -2.6190838401581408E-5,
    3.6899182659531625E-6,
];

fn polynomial(x: f64, coefficients: &[f64]) -> f64 {
    let mut coefficients = coefficients.iter().copied();
    let mut p = coefficients.next().unwrap_or(0.0);
    for c in coefficients {
        p = c + x * p;
    }
    p
}

fn inv_gamma_1p_minus_1(x: f64) -> f64 {
    let t = if x <= 0.5 { x } else { (x - 0.5) - 0.5 };
    if t < 0.0 {
        let a = 6.116095104481416e-9 + t * 6.247308301164655e-9;
        let b = polynomial(t, NEG_B);
        let mut c = -2.056338416977607e-7 + t * (a / b);
        for &v in C {
            c = v + t * c;
        }
        if x > 0.5 {
            t * c / x
        } else {
            x * ((c + 0.5) + 0.5)
        }
    } else {
        let p = polynomial(t, P);
        let q = polynomial(t, Q);
        let mut c = -2.056338416977607e-7 + (p / q) * t;
        for &v in C.iter().take(C.len().saturating_sub(1)) {
            c = v + t * c;
        }
        c = COMMONS_EULER_GAMMA + t * c;
        if x > 0.5 {
            (t / x) * ((c - 0.5) - 0.5)
        } else {
            x * c
        }
    }
}

fn log_gamma_1p(x: f64) -> f64 {
    -inv_gamma_1p_minus_1(x).ln_1p()
}

/// Compute log-gamma using the pinned Commons-compatible approximation.
pub(super) fn log_gamma(x: f64) -> f64 {
    if x < 0.5 {
        return log_gamma_1p(x) - x.ln();
    }
    if x <= 2.5 {
        return log_gamma_1p((x - 0.5) - 0.5);
    }
    if x <= 8.0 {
        let n = (x - 1.5).floor() as usize;
        let mut product = 1.0;
        for i in 1..=n {
            product *= x - i as f64;
        }
        return log_gamma_1p(x - (n + 1) as f64) + product.ln();
    }
    let mut sum = 0.0;
    for i in (1..LANCZOS.len()).rev() {
        // SAFETY: i ranges from one up to but excluding LANCZOS.len().
        sum += LANCZOS[i] / (x + i as f64);
    }
    sum += LANCZOS.first().copied().unwrap_or(0.0);
    let tmp = (x + 4.7421875) + 0.5;
    ((x + 0.5) * tmp.ln() - tmp) + 0.5 * std::f64::consts::TAU.ln() + (sum / x).ln()
}

/// Compute the regularized upper incomplete gamma with metered convergence.
pub(super) fn gamma_q<O, M: FnMut(u64) -> Result<(), O>>(
    a: f64,
    x: f64,
    epsilon: f64,
    limit: usize,
    meter: &mut M,
) -> KernelResult<O> {
    if x == 0.0 {
        return Ok(1.0);
    }
    if x < a + 1.0 {
        return Ok(1.0 - gamma_p(a, x, epsilon, limit, meter)?);
    }
    let small = 1e-50;
    let mut h = (1.0 - a) + x;
    if h.abs() <= small {
        h = small;
    }
    let mut d = 0.0;
    let mut c = h;
    for n in 1..limit {
        step(meter)?;
        let n = n as f64;
        let an = ((2.0 * n + 1.0) - a) + x;
        let bn = n * (a - n);
        let mut dn = an + bn * d;
        if dn.abs() <= small {
            dn = small;
        }
        let mut cn = an + bn / c;
        if cn.abs() <= small {
            cn = small;
        }
        dn = 1.0 / dn;
        let delta = cn * dn;
        let next = h * delta;
        if next.is_infinite() {
            return Err(KernelFailure::SourceConvergence(
                SourceConvergenceFailure::InfiniteContinuedFraction,
            ));
        }
        if next.is_nan() {
            return Err(KernelFailure::SourceConvergence(
                SourceConvergenceFailure::NanContinuedFraction,
            ));
        }
        if (delta - 1.0).abs() < epsilon {
            return Ok(((-x + a * x.ln()) - log_gamma(a)).exp() * (1.0 / next));
        }
        d = dn;
        c = cn;
        h = next;
    }
    Err(KernelFailure::SourceConvergence(
        SourceConvergenceFailure::IterationLimit,
    ))
}

fn gamma_p<O, M: FnMut(u64) -> Result<(), O>>(
    a: f64,
    x: f64,
    epsilon: f64,
    limit: usize,
    meter: &mut M,
) -> KernelResult<O> {
    if x == 0.0 {
        return Ok(0.0);
    }
    if x >= a + 1.0 {
        return Ok(1.0 - gamma_q(a, x, epsilon, limit, meter)?);
    }
    let mut n = 0;
    let mut term = 1.0 / a;
    let mut sum = term;
    while (term / sum).abs() > epsilon && n < limit && sum < f64::INFINITY {
        step(meter)?;
        n += 1;
        term *= x / (a + n as f64);
        sum += term;
    }
    if n >= limit {
        return Err(KernelFailure::SourceConvergence(
            SourceConvergenceFailure::IterationLimit,
        ));
    }
    if sum.is_infinite() {
        return Ok(1.0);
    }
    Ok(((-x + a * x.ln()) - log_gamma(a)).exp() * sum)
}

#[cfg(test)]
mod iteration_tests {
    use super::*;

    #[test]
    fn source_limit_is_distinct_from_budget_failure() {
        let mut accept = |_| Ok::<(), &'static str>(());
        assert_eq!(
            gamma_q(3.0, 1.0, 1e-12, 1, &mut accept),
            Err(KernelFailure::SourceConvergence(
                SourceConvergenceFailure::IterationLimit
            ))
        );
        assert_eq!(
            gamma_q(3.0, 4.0, 1e-12, 1, &mut accept),
            Err(KernelFailure::SourceConvergence(
                SourceConvergenceFailure::IterationLimit
            ))
        );
        let mut reject = |_| Err("budget");
        assert_eq!(
            gamma_q(3.0, 4.0, 1e-12, 10, &mut reject),
            Err(KernelFailure::Operational("budget"))
        );
    }
}
