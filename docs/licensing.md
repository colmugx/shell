# Licensing and third-party boundary

This repository's **own original code and documentation** are offered under **GPL-3.0-or-later** (SPDX identifier). The former MIT root license has been replaced.

Why GPL v3 or later? It permits choosing GPL version 3 or any later FSF-published GPL and is compatible, in the usual applicable conditions, with Apache-2.0 components such as WAMR; GPLv2-only would not have that compatibility.

It does **not** relicense upstream projects, replace their copyright notices, or authorize bundling proprietary game/firmware assets.

| External project | Typical upstream license | Policy |
| --- | --- | --- |
| Linux kernel | GPL-2.0-only, with applicable exceptions | Keep in its own upstream source/build; obey kernel's separate terms |
| WAMR | Apache-2.0 WITH LLVM-exception | No vendored source in this drop; retain upstream notices if later integrated |
| PocketJS | Check licenses per component/version; framework parts publish MIT, other parts may differ | No vendored source in this drop; review before vendoring/linking |
| Buildroot | Primarily GPLv2 or later for its build system; individual packages differ | Keep package notices and license metadata |

GPL is a **copyleft** family, not a permissive license like MIT. 'Or later' is the version-flexible choice; it does not remove the GPL's source-sharing obligations on covered distributions.

Refs: https://www.gnu.org/licenses/gpl-3.0.html ; https://www.gnu.org/licenses/gpl-faq.html ; https://github.com/wasm-micro-runtime/wasm-micro-runtime
