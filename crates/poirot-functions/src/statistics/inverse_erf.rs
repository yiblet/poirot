//! Inverse-error-function polynomial from Apache Commons Math 3.6.1, Apache-2.0.
//! Uses Rust platform ln/sqrt; bitwise identity with Java FastMath is not promised.
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

/// Approximate inverse error function for the normal-inverse kernel.
pub(super) fn inverse_erf(x: f64) -> f64 {
    let mut w = -((1.0 - x) * (1.0 + x)).ln();
    let coefficients: &[f64] = if w < 6.25 {
        w -= 3.125;
        &[
            -3.64441206401782E-21,
            -1.6850591381820166E-19,
            1.28584807152564E-18,
            1.1157877678025181E-17,
            -1.333171662854621E-16,
            2.0972767875968562E-17,
            6.637638134358324E-15,
            -4.054566272975207E-14,
            -8.151934197605472E-14,
            2.6335093153082323E-12,
            -1.2975133253453532E-11,
            -5.415412054294628E-11,
            1.0512122733215323E-9,
            -4.112633980346984E-9,
            -2.9070369957882005E-8,
            4.2347877827932404E-7,
            -1.3654692000834679E-6,
            -1.3882523362786469E-5,
            1.8673420803405714E-4,
            -7.40702534166267E-4,
            -0.006033670871430149,
            0.24015818242558962,
            1.6536545626831027,
        ]
    } else if w < 16.0 {
        w = w.sqrt() - 3.25;
        &[
            2.2137376921775787E-9,
            9.075656193888539E-8,
            -2.7517406297064545E-7,
            1.8239629214389228E-8,
            1.5027403968909828E-6,
            -4.013867526981546E-6,
            2.9234449089955446E-6,
            1.2475304481671779E-5,
            -4.7318229009055734E-5,
            6.828485145957318E-5,
            2.4031110387097894E-5,
            -3.550375203628475E-4,
            9.532893797373805E-4,
            -0.0016882755560235047,
            0.002491442096107851,
            -0.003751208507569241,
            0.005370914553590064,
            1.0052589676941592,
            3.0838856104922208,
        ]
    } else if w.is_finite() {
        w = w.sqrt() - 5.0;
        &[
            -2.7109920616438573E-11,
            -2.555641816996525E-10,
            1.5076572693500548E-9,
            -3.789465440126737E-9,
            7.61570120807834E-9,
            -1.496002662714924E-8,
            2.914795345090108E-8,
            -6.771199775845234E-8,
            2.2900482228026655E-7,
            -9.9298272942317E-7,
            4.526062597223154E-6,
            -1.968177810553167E-5,
            7.599527703001776E-5,
            -2.1503011930044477E-4,
            -1.3871931833623122E-4,
            1.0103004648645344,
            4.849906401408584,
        ]
    } else {
        return f64::INFINITY * x;
    };
    // SAFETY: Each branch above constructs a nonempty literal coefficient array, so element zero and the suffix from one exist.
    let mut p = coefficients[0];
    // SAFETY: Each branch above constructs a nonempty literal coefficient array, so element zero and the suffix from one exist.
    for &c in &coefficients[1..] {
        p = c + p * w;
    }
    p * x
}
