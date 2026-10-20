/* ====================================================================
   Licensed to the Apache Software Foundation (ASF) under one or more
   contributor license agreements.  See the LICENSES/ACKNOWLEDGEMENTS file distributed with
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

//! Commons Math 3.6.1 PoissonDistribution/SaddlePointExpansion, Apache-2.0.
//! Operation order and constants follow authenticated bytecode; platform ln/exp remain.
use crate::iteration::{KernelResult, step};
const STIRLING: &[f64] = &[
    0.0,
    0.15342640972002736,
    0.08106146679532726,
    0.05481412105191765,
    0.0413406959554093,
    0.03316287351993629,
    0.02767792568499834,
    0.023746163656297496,
    0.020790672103765093,
    0.018488450532673187,
    0.016644691189821193,
    0.015134973221917378,
    0.013876128823070748,
    0.012810465242920227,
    0.01189670994589177,
    0.011104559758206917,
    0.010411265261972096,
    0.009799416126158804,
    0.009255462182712733,
    0.008768700134139386,
    0.00833056343336287,
    0.00793411456431402,
    0.007573675487951841,
    0.007244554301320383,
    0.00694284010720953,
    0.006665247032707682,
    0.006408994188004207,
    0.006171712263039458,
    0.0059513701127588475,
    0.0057462165130101155,
    0.005554733551962801,
];

fn stirling_error(x: f64) -> f64 {
    if x < 15.0 {
        return STIRLING
            .get((2.0 * x) as usize)
            .copied()
            .unwrap_or(f64::NAN);
    }
    let xx = x * x;
    (0.08333333333333333
        - (0.002777777777777778
            - (7.936507936507937e-4 - (5.952380952380953e-4 - 8.417508417508417e-4 / xx) / xx)
                / xx)
            / xx)
        / x
}

fn deviance<O, M: FnMut(u64) -> Result<(), O>>(
    x: f64,
    mean: f64,
    meter: &mut M,
) -> KernelResult<O> {
    if (x - mean).abs() < 0.1 * (x + mean) {
        let d = x - mean;
        let v = d / (x + mean);
        let mut sum = v * d;
        let mut previous = f64::NAN;
        let mut term = (2.0 * x) * v;
        let vv = v * v;
        let mut j = 1;
        while sum != previous {
            step(meter)?;
            previous = sum;
            term *= vv;
            sum = previous + term / (2 * j + 1) as f64;
            j += 1;
        }
        Ok(sum)
    } else {
        Ok(x * (x / mean).ln() + mean - x)
    }
}

/// Compute one Poisson probability mass using stable source-compatible arithmetic.
pub(super) fn probability<O, M: FnMut(u64) -> Result<(), O>>(
    count: u64,
    mean: f64,
    meter: &mut M,
) -> KernelResult<O> {
    if count == i32::MAX as u64 {
        return Ok(0.0);
    }
    if count == 0 {
        return Ok((-mean).exp());
    }
    let x = count as f64;
    Ok(
        (((-stirling_error(x) - deviance(x, mean, meter)?) - 0.5 * std::f64::consts::TAU.ln())
            - 0.5 * x.ln())
        .exp(),
    )
}
