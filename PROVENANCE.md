# PROVENANCE

Authored by Riaan, Invariant Forge — riaan@invariantforge.net

This crate is **independently authored forensic / verification tooling**. It originated from the evidence
method first written as `gnucobol-rs/lab/` to validate the `gnucobol-rs` claims, and is maintained as the
author's own work.

It is **not GnuCOBOL**, not a fork of GnuCOBOL, and not a GnuCOBOL runtime or compiler. It does **not** copy
or translate GnuCOBOL implementation source.

Where this tooling interacts with GnuCOBOL at all, it invokes an externally installed `cobc`/`libcob`
toolchain as an **external oracle** and records observed behavior — or it consumes the already-committed
evidence artifacts produced that way. It does not vendor GnuCOBOL source code and does not distribute
modified GnuCOBOL binaries unless a specific distribution package separately documents and satisfies the
applicable GPL/LGPL obligations.

Its artifacts — courts, casefiles, receipts, atlases, fuzz campaigns, Merkle seals, invariant rankings,
compliance receipts, and support packets — are independently authored evidence products.

The license-boundary posture of this crate is continuously checked by the independent auditor
`gpl-license-guard` (github.com/infinityabundance/gpl-license-guard), whose receipt is committed as evidence.
