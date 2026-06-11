# DEPENDENCY POLICY

Authored by Riaan, Invariant Forge — riaan@invariantforge.net

This policy keeps the GPL/LGPL boundary clean. It is enforced mechanically by the independent
`gpl-license-guard` auditor (committed receipt), not by assertion.

1. A commercial crate MUST NOT depend on, link, or vendor GPL-only GnuCOBOL implementation code.
2. This tooling MAY invoke `cobc` as an external executable (process boundary — the cleanest posture).
3. This tooling MAY consume stdout/stderr/output files produced by externally installed tools.
4. This tooling MAY depend on the `gnucobol-rs` crate only where its LGPL-3.0-or-later license permits the
   intended use; any distributed binary that links it honors the LGPL relink obligation.
5. `gnucobol-rs` MUST NOT depend on any of these crates (runtime). They are dev/CI/evidence consumers only.
6. Any distribution that bundles GnuCOBOL/libcob MUST include a separate GPL/LGPL compliance manifest.
7. Public COBOL corpora are never vendored into a commercial package unless their own license allows it.
