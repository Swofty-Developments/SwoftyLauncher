# T340 follow-up run 20261126-164110

Actual 0.3.58 logging test, Steam 26566308 / Swofty 1.10.53. The user reports: "Done, everything worked." `trace.log` contains exact complete selected lines from the immutable loader-log snapshot; SOURCE_LINES.json maps them to original source line numbers. The full snapshot is retained outside the PR. This snapshot may precede launcher archival; no session-end or crash-free claim is inferred from a file not yet finalized.

Observed: 10 normal preset passthrough call pairs, 2 completed fit relay call pairs, 8 active-owner deactivation triples and 7 destructor triples. All matched by tag/self/thread; command name/data pointers and hashes agree; all fitting returns are normal with faulted=0. Deactivation records tracked_before=1 -> tracked_after=0 before native return. Destruction forwards the observed flags=0 and returns self, with an already-untracked owner. Nonzero deletion/free behavior is not runtime-confirmed.

`trace:T340/20261126-164110:N` refers to line N of this trace.log. T343 and T344 share the same physical run and cite these exact lines. Reused 0.3.57 test launcher and current public-name mixins; no private fallback or additional hook. Retained artifact hashes are distinguished from runtime DLL hash attestation in PROVENANCE.json.
