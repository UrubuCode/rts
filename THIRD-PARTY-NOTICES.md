# Third-party notices

RTS is MIT licensed (see `LICENSE`). This file records material from other works
that RTS documentation or source is derived from, and the terms that come with
it.

---

## ECMA-262 — the ECMAScript Language Specification

**Where it is used.** `crates/rts-codegen/PLAN.md` was written against the
specification's own source: the grammar inventory in its §2 was extracted from
`spec.html` in the `tc39/ecma262` repository, and its §2b and §5 paraphrase
grammar rules and runtime-semantics algorithms defined there.

**Derivation statement**, per condition (iii) of the licence reproduced below:

> This document includes material copied from or derived from the ECMAScript®
> 2027 Language Specification https://tc39.es/ecma262/.
> Copyright © Ecma International.

**Scope.** Copyright covers the specification *text*. Implementing the language
it describes is not a use of that text and is not restricted by it. What this
notice discharges is the redistribution of extracted and paraphrased material in
our own documentation.

**Trademark.** "ECMAScript" is a registered trademark of Ecma International.
Under the licence terms below, the name and trademarks of the copyright holders
may not be used in advertising or publicity relating to this work without prior
written permission. RTS therefore describes what it implements, and does not
claim endorsement, certification, or affiliation.

### Copyright notice and copyright licence

> ## Copyright Notice
>
> ALTERNATIVE COPYRIGHT NOTICE AND COPYRIGHT LICENSE
>
> © Ecma International 2026
>
> By obtaining and/or copying this work, you (the licensee) agree that you have
> read, understood, and will comply with the following terms and conditions.
>
> Permission, under Ecma's copyright, to copy, modify, and prepare derivative
> works of, and distribute this work, with or without modification, for any
> purpose and without fee or royalty is hereby granted, provided that you
> include the following on ALL copies of the work or portions thereof, including
> modifications:
>
> (i) The full text of this COPYRIGHT NOTICE AND COPYRIGHT LICENSE in a location
> viewable to users of the redistributed or derivative work.
>
> (ii) Any pre-existing intellectual property disclaimers, notices, or terms and
> conditions. If none exist, the Ecma alternative copyright notice should be
> included.
>
> (iii) Notice of any changes or modifications, through a copyright statement on
> the document such as "This document includes material copied from or derived
> from the ECMAScript® 2027 Language Specification
> https://tc39.es/ecma262/. Copyright © Ecma International."
>
> ## Disclaimers
>
> THIS WORK IS PROVIDED "AS IS," AND COPYRIGHT HOLDERS MAKE NO REPRESENTATIONS
> OR WARRANTIES, EXPRESS OR IMPLIED, INCLUDING, BUT NOT LIMITED TO, WARRANTIES
> OF MERCHANTABILITY OR FITNESS FOR ANY PARTICULAR PURPOSE OR THAT THE USE OF
> THE DOCUMENT WILL NOT INFRINGE ANY THIRD PARTY PATENTS, COPYRIGHTS, TRADEMARKS
> OR OTHER RIGHTS.
>
> COPYRIGHT HOLDERS WILL NOT BE LIABLE FOR ANY DIRECT, INDIRECT, SPECIAL OR
> CONSEQUENTIAL DAMAGES ARISING OUT OF ANY USE OF THE DOCUMENT.
>
> The name and trademarks of copyright holders may NOT be used in advertising or
> publicity pertaining to the work without specific, written prior permission.
> Title to copyright in this work will at all times remain with copyright
> holders.
>
> ## Software License
>
> All Software contained in this document ("Software") is protected by copyright
> and is being made available under the "BSD License", included below. This
> Software may be subject to third party rights (rights from parties other than
> Ecma International), including patent rights, and no licenses under such third
> party rights are granted under this license even if the third party concerned
> is a member of Ecma International. SEE THE ECMA CODE OF CONDUCT IN PATENT
> MATTERS AVAILABLE AT https://ecma-international.org/memento/codeofconduct.htm
> FOR INFORMATION REGARDING THE LICENSING OF PATENT CLAIMS THAT ARE REQUIRED TO
> IMPLEMENT ECMA INTERNATIONAL STANDARDS.
>
> Redistribution and use in source and binary forms, with or without
> modification, are permitted provided that the following conditions are met:
>
> 1. Redistributions of source code must retain the above copyright notice, this
>    list of conditions and the following disclaimer.
> 2. Redistributions in binary form must reproduce the above copyright notice,
>    this list of conditions and the following disclaimer in the documentation
>    and/or other materials provided with the distribution.
> 3. Neither the name of the authors nor Ecma International may be used to
>    endorse or promote products derived from this software without specific
>    prior written permission.
>
> THIS SOFTWARE IS PROVIDED BY ECMA INTERNATIONAL "AS IS" AND ANY EXPRESS OR
> IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF
> MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO
> EVENT SHALL ECMA INTERNATIONAL BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
> SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
> PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR
> BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER
> IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
> ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
> POSSIBILITY OF SUCH DAMAGE.

### Working rules for this repository

1. **Do not vendor `spec.html`.** It is read from a working clone outside the
   repository. Nothing about the audit requires the file to be committed, and not
   committing it keeps the redistribution question from arising at all.
2. **Quote sparingly, cite always.** Refer to an operation by its name and
   section id (`IsLessThan`, `sec-islessthan`) rather than reproducing its steps.
   Where a rule has to be stated, state it in our own words.
3. **Any file deriving from the specification carries the (iii) statement** and
   points here.
4. **No endorsement claims.** Describe conformance as measured; never as
   certified or approved.

---

## Other works consulted

- **ESTree** (`estree/estree`) — read as a second, independent inventory of AST
  node kinds and operator sets while auditing for omissions. No text or
  structure was copied; RTS's tree deliberately differs. Check the project's own
  licence before reproducing any of its material here.
- **test262** (`tc39/test262`) — planned as the coverage measurement in phase L9.
  It carries its own licence, separate from ECMA-262's. If test files or their
  content are ever vendored or reproduced, that licence must be reviewed and
  recorded in this file first.
