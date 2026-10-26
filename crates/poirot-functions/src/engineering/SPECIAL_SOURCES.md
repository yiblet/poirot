# Real engineering special functions

`bessel_k.rs` translates only the binary64 (53-bit) branches of Boost.Math
1.85.0 `bessel_k0.hpp` and `bessel_k1.hpp`, immutable revision
`434a017db4c7abc115d8a8ae08a446b70feb34f2`:

- https://github.com/boostorg/math/blob/434a017db4c7abc115d8a8ae08a446b70feb34f2/include/boost/math/special_functions/detail/bessel_k0.hpp
- https://github.com/boostorg/math/blob/434a017db4c7abc115d8a8ae08a446b70feb34f2/include/boost/math/special_functions/detail/bessel_k1.hpp

Copyright (c) 2006 Xiaogang Zhang; Copyright (c) 2017 John Maddock.
The Boost Software License 1.0 applies; its complete text is packaged beside
this file in `BOOST_LICENSE_1_0.txt`. Poirot's MIT license does not replace it.
Upstream explicitly identifies these coefficients as Maddock's own derivation.
No Numerical Recipes or separately licensed website coefficients were reused.
SHA256 of upstream files, respectively:
`fe3d5c9576dadf3326f0d8f1f34d3b27c681ce2c891d825a0bd2a49d263f241e`,
`b44ba71e0f40ff3ed8871ad6fecca0112c22a83db610cb5c847fe70a74d6cfd1`.

Adaptation removes generic C++ dispatch/policies/initialization and exposes the
logarithm of each exponentially scaled positive base value. The final exponential
is restored only after the integer-order recurrence; for small x, K1 is formed as x*K1 before taking
the logarithm, avoiding intermediate overflow. Horner evaluation is independently
written. No upstream rational helper, tests or fixtures were copied.
Higher integer K orders use independently implemented binary-scaled positive
recurrence from DLMF 10.29, retaining a separate integer exponent. I uses an
independently implemented positive defining
series from DLMF 10.25.2, initialized at its largest term so the starting value
cannot underflow before later terms recover. Both series tails decrease;
the forward tail uses a geometric remainder bound. libm 0.2.16, already an MIT
workspace dependency, supplies lgamma, J/Y and erf/erfc APIs without source copying.

Mathematical sources: https://dlmf.nist.gov/10.25 ,
https://dlmf.nist.gov/10.29 , https://dlmf.nist.gov/7.2 .
These mathematical definitions are not borrowed implementation code or fixtures.
Integer orders above 100,000 and I arguments above 100,000 receive operational
capability refusals, not worksheet values. Every recurrence/series step is
metered; J/Y receive a conservative precharge. Numerical overflow is a numeric
domain failure; convergence/envelope exhaustion is operational. The modal I seed
uses a rearranged Stirling expansion (DLMF 5.11) for large
factorials and libm lgamma for small ones. Final exponential normalization at
large orders can still lose precision through cancellation. Independently
computed boundary cases at order 100,000 show relative errors around 3e-11 for I
and 4e-12 for K; this is measured evidence, not a guarantee for every input or a
claim of native Excel bit identity.
