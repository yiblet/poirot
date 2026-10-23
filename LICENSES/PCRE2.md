# PCRE2 dependencies

Poirot uses the safe `pcre2` Rust API to implement Microsoft's PCRE2 regex
functions. Its implementation and tests are independently authored. Global
matching follows the documented PCRE2 empty-match iteration rule through the
NOTEMPTY_ATSTART directive. No upstream source, test, or fixture was copied.
The semantic rule comes from the PCRE2 10.46 manual:
https://github.com/PCRE2Project/pcre2/blob/pcre2-10.46/doc/pcre2pattern.3
and the API description: https://www.pcre.org/current/doc/html/pcre2api.html .

- `pcre2` 0.2.11, MIT option of `Unlicense OR MIT`:
  https://github.com/BurntSushi/rust-pcre2/tree/9f269262a71d88605c67bd3fcc8633970aa6ad81
- `pcre2-sys` 0.2.10, MIT option of `Unlicense OR MIT`:
  https://github.com/BurntSushi/rust-pcre2/tree/fd05026e1bf3ad62b3876cf9bd952dc742368462/pcre2-sys
- Its bundled PCRE2 is version 10.46 (2025-08-27), under `BSD-3-Clause WITH PCRE2-exception`; the exception relaxes binary
  redistribution obligations. The package also retains the older compatible
  BSD-style University of Cambridge notices in its source headers. Source provenance is
  the `pcre2-sys` revision above, `upstream/include/pcre2.h` and `upstream/src`.
  Upstream: https://github.com/PCRE2Project/pcre2/releases/tag/pcre2-10.46
  Authoritative license: https://github.com/PCRE2Project/pcre2/blob/pcre2-10.46/LICENCE.md
- The bundled SLJIT source retains Zoltan Herczeg's BSD-style notice.
  Provenance: the same sys revision, `upstream/deps/sljit/sljit_src/sljitLir.c`.

The MIT and native notices are preserved in `PCRE2-notices.txt`. Cargo obtains
the dependency sources with their original notices. `pcre2-sys` normally links
an available system `libpcre2-8`, otherwise builds its bundled version. Set
`PCRE2_SYS_STATIC=1` when a reproducible bundled build is needed. This repository's
validation environment links system PCRE2 10.46, verified with `pkg-config` and the
linked library; no native code or unsafe Rust is added to Poirot targets.

Additional locked build dependencies (`cc`, `find-msvc-tools`, `jobserver`,
`pkg-config`, `shlex`, and target-specific `getrandom`/`r-efi`) have permissive
MIT options in their published Cargo manifests; select MIT for each (including
the MIT option of `r-efi`'s MIT/Apache-2.0/LGPL alternatives). Their source
packages retain their notices. They are dependency tools, not copied source.

The 8-bit safe API works with valid Unicode strings. Poirot rejects isolated
UTF-16 surrogates and active `\C` code-unit escapes operationally rather than
converting text lossily or returning a different Boolean result. Native match,
depth and heap limits are operational failures and cannot be caught by IFERROR.
Native Excel's exact PCRE2 version/options, obscure replacement spellings,
variable-length extraction over array inputs, and unsupported empty-match
continuations are compatibility audit items, not claims of full equivalence.
