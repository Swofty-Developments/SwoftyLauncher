# Shared tab-model identity through a registered code site

The formal record is `re/bindings/vm/site_Vm_SharedTabModelVtable.json`, created with Swofty's existing, unchanged `binding.py new` command:

```sh
python re/tools/binding.py new site_Vm_SharedTabModelVtable 0x140c72e36 --kind site --match-from 0x140c72e2b --expect-len 3 --id vm.shared_tab_model_vtable --author UnorthodoxStoneND --exe /path/to/current/F1_25.exe --evidence "decomp:own disasm 0x140c72940-0x140c72e60;xref:0x140c72e36,0x140ce5af9;doc:re/research/garage-monitor-current-model/README.md"
```

The command maps the current instruction to KB 0x140c72376 using the two existing bracketing bindings, both shifted by +0xac0. It writes and proves the record normally. No table-address twin is invented, no registration tool or protocol is modified, and no old executable is used.

The site's expected bytes are `48 8D 05`: `LEA RAX,[RIP+disp32]`. Its vtable operand is the signed 32-bit value at site+3 plus site+7. The consumer resolves the registered site through Swofty's API, safely reads seven bytes, checks the opcode and arithmetic bounds, then uses the derived table address for the existing typed-model guard. It performs no private pattern scan and installs no additional hook. Missing/unreadable/changed instructions retain the separate Vehicle tab by failing closed. The native model lifecycle, controls, fitting and visibility call are unchanged.

On Steam 26566308 / F1 25 1.0.145.2472 the unchanged resolver finds match 0x140c72e2b, site 0x140c72e36, and the operand yields model table 0x14568f580. The earlier T339 trace line 26 records this same pattern match and table under the old candidate name; line 49 records the successful typed-model native visibility operation. That is historical target-identity evidence, not a claim that the newly registered site consumer has been run in-game. The new record stays L2 pending direct runtime confirmation or normal independent verification. Full table extent and other-build runtime behavior are not claimed.

`EVIDENCE.json` retains the earlier static model-table investigation and eighteen references. The old test-catalogue candidate is kept outside the submission. The formal site record is the proposed API dependency. Local native fixtures test positive and negative displacement, absent binding, bad opcode and unreadable bytes, while preserving the original six mixins, command/lifecycle tests and actual DLL fixtures. Everything remains local; nothing installed or pushed.
