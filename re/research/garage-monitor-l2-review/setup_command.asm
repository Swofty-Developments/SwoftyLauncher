0x140c1ce20: push rsi
0x140c1ce21: push rdi
0x140c1ce22: push rbx
0x140c1ce23: sub rsp, 0xf0
0x140c1ce2a: mov rax, qword ptr [rdx]
0x140c1ce2d: test rax, rax
0x140c1ce30: je 0x140c1d497
0x140c1ce36: mov rsi, rcx
0x140c1ce39: mov eax, dword ptr [rax]
0x140c1ce3b: cmp eax, 0x220648c
0x140c1ce40: jle 0x140c1ce7d
0x140c1ce42: cmp eax, 0x713fcaca
0x140c1ce47: jg 0x140c1d03c
0x140c1ce4d: cmp eax, 0x220648d
0x140c1ce52: je 0x140c1d113
0x140c1ce58: cmp eax, 0x6ffbb046
0x140c1ce5d: jne 0x140c1d497
0x140c1ce63: mov rcx, rsi
0x140c1ce66: call 0x140c1d4b0
0x140c1ce6b: mov rcx, rsi
0x140c1ce6e: add rsp, 0xf0
0x140c1ce75: pop rbx
0x140c1ce76: pop rdi
0x140c1ce77: pop rsi
0x140c1ce78: jmp 0x140c1c490
0x140c1ce7d: cmp eax, 0xcae30fad
0x140c1ce82: je 0x140c1d214
0x140c1ce88: cmp eax, 0xcb30db90
0x140c1ce8d: je 0x140c1d315
0x140c1ce93: cmp eax, 0xfe6cc979
0x140c1ce98: jne 0x140c1d497
0x140c1ce9e: mov byte ptr [rsp + 0x38], 0
0x140c1cea3: mov rdi, qword ptr [rsi + 0x7da0]
0x140c1ceaa: test rdi, rdi
0x140c1cead: je 0x140c1cf08
0x140c1ceaf: cmp rdi, qword ptr [rip + 0x543995a]
0x140c1ceb6: je 0x140c1cf08
0x140c1ceb8: test byte ptr [rdi + 0x26], 1
0x140c1cebc: jne 0x140c1cf08
0x140c1cebe: mov rax, qword ptr [rdi]
0x140c1cec1: mov rcx, rdi
0x140c1cec4: call qword ptr [rax + 0x20]
0x140c1cec7: test al, al
0x140c1cec9: je 0x140c1cf08
0x140c1cecb: mov rcx, qword ptr [rdi + 0x58]
0x140c1cecf: mov rax, qword ptr [rcx]
0x140c1ced2: call qword ptr [rax + 8]
0x140c1ced5: mov rdx, qword ptr [rax]
0x140c1ced8: mov rcx, rax
0x140c1cedb: call qword ptr [rdx + 0x10]
0x140c1cede: mov ebx, eax
0x140c1cee0: mov rax, qword ptr [rip + 0x550eed9]
0x140c1cee7: lea rcx, [rip + 0x550eed2]
0x140c1ceee: call qword ptr [rax + 0x10]
0x140c1cef1: cmp ebx, eax
0x140c1cef3: jne 0x140c1cf08
0x140c1cef5: lea rdx, [rsp + 0x38]
0x140c1cefa: mov rcx, rdi
0x140c1cefd: xor r8d, r8d
0x140c1cf00: xor r9d, r9d
0x140c1cf03: call 0x140973e70
0x140c1cf08: mov byte ptr [rsp + 0x38], 0
0x140c1cf0d: mov rdi, qword ptr [rsi + 0x7dc0]
0x140c1cf14: test rdi, rdi
0x140c1cf17: je 0x140c1cf72
0x140c1cf19: cmp rdi, qword ptr [rip + 0x54398f0]
0x140c1cf20: je 0x140c1cf72
0x140c1cf22: test byte ptr [rdi + 0x26], 1
0x140c1cf26: jne 0x140c1cf72
0x140c1cf28: mov rax, qword ptr [rdi]
0x140c1cf2b: mov rcx, rdi
0x140c1cf2e: call qword ptr [rax + 0x20]
0x140c1cf31: test al, al
0x140c1cf33: je 0x140c1cf72
0x140c1cf35: mov rcx, qword ptr [rdi + 0x58]
0x140c1cf39: mov rax, qword ptr [rcx]
0x140c1cf3c: call qword ptr [rax + 8]
0x140c1cf3f: mov rdx, qword ptr [rax]
0x140c1cf42: mov rcx, rax
0x140c1cf45: call qword ptr [rdx + 0x10]
0x140c1cf48: mov ebx, eax
0x140c1cf4a: mov rax, qword ptr [rip + 0x550ee6f]
0x140c1cf51: lea rcx, [rip + 0x550ee68]
0x140c1cf58: call qword ptr [rax + 0x10]
0x140c1cf5b: cmp ebx, eax
0x140c1cf5d: jne 0x140c1cf72
0x140c1cf5f: lea rdx, [rsp + 0x38]
0x140c1cf64: mov rcx, rdi
0x140c1cf67: xor r8d, r8d
0x140c1cf6a: xor r9d, r9d
0x140c1cf6d: call 0x140973e70
0x140c1cf72: mov rcx, qword ptr [rsi + 0x7e78]
0x140c1cf79: xor edx, edx
0x140c1cf7b: call 0x1424ae040
0x140c1cf80: test al, al
0x140c1cf82: je 0x140c1d429
0x140c1cf88: mov rcx, qword ptr [rsi + 0x7e50]
0x140c1cf8f: mov rax, qword ptr [rcx + 0x10]
0x140c1cf93: add rcx, 0x10
0x140c1cf97: call qword ptr [rax + 0x58]
0x140c1cf9a: lea rcx, [rip + 0x87f]
0x140c1cfa1: mov qword ptr [rsp + 0xd0], rcx
0x140c1cfa9: mov qword ptr [rsp + 0xd8], rsi
0x140c1cfb1: lea rcx, [rsp + 0x50]
0x140c1cfb6: mov qword ptr [rsp + 0x40], 0x80
0x140c1cfbf: mov qword ptr [rsp + 0x48], rcx
0x140c1cfc4: vmovaps xmm0, xmmword ptr [rip + 0x4a56df4]
0x140c1cfcc: vmovups xmmword ptr [rsp + 0x50], xmm0
0x140c1cfd2: vmovaps xmm0, xmmword ptr [rip + 0x4a56df6]
0x140c1cfda: vmovups xmmword ptr [rsp + 0x60], xmm0
0x140c1cfe0: mov word ptr [rsp + 0x70], 0x67
0x140c1cfe7: lea rcx, [rip + 0x49f8682]
0x140c1cfee: mov qword ptr [rsp + 0x38], rcx
0x140c1cff3: vxorps xmm0, xmm0, xmm0
0x140c1cff7: vmovups xmmword ptr [rsp + 0x20], xmm0
0x140c1cffd: lea rdi, [rsp + 0xe0]
0x140c1d005: lea r8, [rsp + 0x38]
0x140c1d00a: lea r9, [rsp + 0xd0]
0x140c1d012: mov rcx, rax
0x140c1d015: mov rdx, rdi
0x140c1d018: call 0x1442dc1c0
0x140c1d01d: add rsi, 0x7e90
0x140c1d024: mov rcx, rsi
0x140c1d027: mov rdx, rdi
0x140c1d02a: call 0x1442acc90
0x140c1d02f: mov rcx, rdi
0x140c1d032: call 0x1442acf50
0x140c1d037: jmp 0x140c1d497
0x140c1d03c: cmp eax, 0x713fcacb
0x140c1d041: je 0x140c1d417
0x140c1d047: cmp eax, 0x7e69bdef
0x140c1d04c: jne 0x140c1d497
0x140c1d052: mov rcx, rsi
0x140c1d055: call 0x140c1b990
0x140c1d05a: cmp byte ptr [rip + 0x543f917], 0
0x140c1d061: jne 0x140c1d497
0x140c1d067: mov rcx, qword ptr [rsi + 0x7e50]
0x140c1d06e: mov rax, qword ptr [rcx + 0x10]
0x140c1d072: add rcx, 0x10
0x140c1d076: call qword ptr [rax + 0x58]
0x140c1d079: lea rcx, [rsp + 0x50]
0x140c1d07e: mov qword ptr [rsp + 0x40], 0x80
0x140c1d087: mov qword ptr [rsp + 0x48], rcx
0x140c1d08c: vmovaps xmm0, xmmword ptr [rip + 0x4a56d1c]
0x140c1d094: vmovups xmmword ptr [rsp + 0x50], xmm0
0x140c1d09a: movabs rcx, 0x67736d5f73676e69
0x140c1d0a4: mov qword ptr [rsp + 0x60], rcx
0x140c1d0a9: mov byte ptr [rsp + 0x68], 0
0x140c1d0ae: lea rcx, [rip + 0x49f85bb]
0x140c1d0b5: mov qword ptr [rsp + 0x38], rcx
0x140c1d0ba: vxorps xmm0, xmm0, xmm0
0x140c1d0be: vmovaps xmmword ptr [rsp + 0xe0], xmm0
0x140c1d0c7: vmovups xmmword ptr [rsp + 0x20], xmm0
0x140c1d0cd: lea rdi, [rsp + 0xd0]
0x140c1d0d5: lea r8, [rsp + 0x38]
0x140c1d0da: lea r9, [rsp + 0xe0]
0x140c1d0e2: mov rcx, rax
0x140c1d0e5: mov rdx, rdi
0x140c1d0e8: call 0x1442dc1c0
0x140c1d0ed: add rsi, 0x7e18
0x140c1d0f4: mov rcx, rsi
0x140c1d0f7: mov rdx, rdi
0x140c1d0fa: call 0x1442acc90
0x140c1d0ff: mov rcx, rdi
0x140c1d102: call 0x1442acf50
0x140c1d107: mov byte ptr [rip + 0x543f86a], 1
0x140c1d10e: jmp 0x140c1d497
0x140c1d113: mov rcx, rsi
0x140c1d116: call 0x140c1c490
0x140c1d11b: mov rax, qword ptr [rsi + 0x268]
0x140c1d122: xor edx, edx
0x140c1d124: mov r8d, 0x9000001
0x140c1d12a: mov r9d, 0x12000002
0x140c1d130: xor ecx, ecx
0x140c1d132: nop word ptr cs:[rax + rax]
0x140c1d140: lea r10d, [rcx - 7]
0x140c1d144: cmp r10d, 0xb
0x140c1d148: jb 0x140c1d155
0x140c1d14a: cmp ecx, 0x1c
0x140c1d14d: ja 0x140c1d159
0x140c1d14f: bt r9d, ecx
0x140c1d153: jae 0x140c1d159
0x140c1d155: bts rdx, rcx
0x140c1d159: lea r10d, [rcx - 6]
0x140c1d15d: cmp r10d, 0xb
0x140c1d161: jb 0x140c1d16e
0x140c1d163: cmp ecx, 0x1b
0x140c1d166: ja 0x140c1d17a
0x140c1d168: bt r8d, ecx
0x140c1d16c: jae 0x140c1d17a
0x140c1d16e: mov r10d, 2
0x140c1d174: shl r10, cl
0x140c1d177: or rdx, r10
0x140c1d17a: lea r10d, [rcx - 5]
0x140c1d17e: cmp r10d, 0xb
0x140c1d182: jb 0x140c1d194
0x140c1d184: lea r10d, [rcx + 1]
0x140c1d188: cmp r10d, 0x1b
0x140c1d18c: ja 0x140c1d1a0
0x140c1d18e: bt r8d, r10d
0x140c1d192: jae 0x140c1d1a0
0x140c1d194: mov r10d, 4
0x140c1d19a: shl r10, cl
0x140c1d19d: or rdx, r10
0x140c1d1a0: add rcx, 3
0x140c1d1a4: cmp rcx, 0x2a
0x140c1d1a8: jne 0x140c1d140
0x140c1d1aa: mov rcx, qword ptr [rax + 8]
0x140c1d1ae: not rcx
0x140c1d1b1: test rdx, rcx
0x140c1d1b4: je 0x140c1d497
0x140c1d1ba: cmp byte ptr [rsi + 0x7e6a], 0
0x140c1d1c1: jne 0x140c1d1f7
0x140c1d1c3: cmp qword ptr [rsi + 0x7c80], 0
0x140c1d1cb: jne 0x140c1d1f7
0x140c1d1cd: lea rdi, [rsi + 0x7678]
0x140c1d1d4: mov rcx, qword ptr [rsi + 0x7e88]
0x140c1d1db: call 0x140ac1420
0x140c1d1e0: mov rcx, rdi
0x140c1d1e3: mov dl, 5
0x140c1d1e5: mov r8, rax
0x140c1d1e8: xor r9d, r9d
0x140c1d1eb: call 0x140c168d0
0x140c1d1f0: mov rax, qword ptr [rsi + 0x268]
0x140c1d1f7: add rsi, 0x7678
0x140c1d1fe: movzx edx, byte ptr [rax + 1]
0x140c1d202: mov rcx, rsi
0x140c1d205: add rsp, 0xf0
0x140c1d20c: pop rbx
0x140c1d20d: pop rdi
0x140c1d20e: pop rsi
0x140c1d20f: jmp 0x140c17d10
0x140c1d214: mov byte ptr [rsp + 0x38], 1
0x140c1d219: mov rdi, qword ptr [rsi + 0x7de0]
0x140c1d220: test rdi, rdi
0x140c1d223: je 0x140c1d27e
0x140c1d225: cmp rdi, qword ptr [rip + 0x54395e4]
0x140c1d22c: je 0x140c1d27e
0x140c1d22e: test byte ptr [rdi + 0x26], 1
0x140c1d232: jne 0x140c1d27e
0x140c1d234: mov rax, qword ptr [rdi]
0x140c1d237: mov rcx, rdi
0x140c1d23a: call qword ptr [rax + 0x20]
0x140c1d23d: test al, al
0x140c1d23f: je 0x140c1d27e
0x140c1d241: mov rcx, qword ptr [rdi + 0x58]
0x140c1d245: mov rax, qword ptr [rcx]
0x140c1d248: call qword ptr [rax + 8]
0x140c1d24b: mov rdx, qword ptr [rax]
0x140c1d24e: mov rcx, rax
0x140c1d251: call qword ptr [rdx + 0x10]
0x140c1d254: mov ebx, eax
0x140c1d256: mov rax, qword ptr [rip + 0x550eb63]
0x140c1d25d: lea rcx, [rip + 0x550eb5c]
0x140c1d264: call qword ptr [rax + 0x10]
0x140c1d267: cmp ebx, eax
0x140c1d269: jne 0x140c1d27e
0x140c1d26b: lea rdx, [rsp + 0x38]
0x140c1d270: mov rcx, rdi
0x140c1d273: xor r8d, r8d
0x140c1d276: xor r9d, r9d
0x140c1d279: call 0x140973e70
0x140c1d27e: mov byte ptr [rsp + 0x38], 1
0x140c1d283: mov rdi, qword ptr [rsi + 0x7e00]
0x140c1d28a: test rdi, rdi
0x140c1d28d: je 0x140c1d2e8
0x140c1d28f: cmp rdi, qword ptr [rip + 0x543957a]
0x140c1d296: je 0x140c1d2e8
0x140c1d298: test byte ptr [rdi + 0x26], 1
0x140c1d29c: jne 0x140c1d2e8
0x140c1d29e: mov rax, qword ptr [rdi]
0x140c1d2a1: mov rcx, rdi
0x140c1d2a4: call qword ptr [rax + 0x20]
0x140c1d2a7: test al, al
0x140c1d2a9: je 0x140c1d2e8
0x140c1d2ab: mov rcx, qword ptr [rdi + 0x58]
0x140c1d2af: mov rax, qword ptr [rcx]
0x140c1d2b2: call qword ptr [rax + 8]
0x140c1d2b5: mov rdx, qword ptr [rax]
0x140c1d2b8: mov rcx, rax
0x140c1d2bb: call qword ptr [rdx + 0x10]
0x140c1d2be: mov ebx, eax
0x140c1d2c0: mov rax, qword ptr [rip + 0x550eaf9]
0x140c1d2c7: lea rcx, [rip + 0x550eaf2]
0x140c1d2ce: call qword ptr [rax + 0x10]
0x140c1d2d1: cmp ebx, eax
0x140c1d2d3: jne 0x140c1d2e8
0x140c1d2d5: lea rdx, [rsp + 0x38]
0x140c1d2da: mov rcx, rdi
0x140c1d2dd: xor r8d, r8d
0x140c1d2e0: xor r9d, r9d
0x140c1d2e3: call 0x140973e70
0x140c1d2e8: mov rcx, rsi
0x140c1d2eb: call 0x140c1b990
0x140c1d2f0: mov dword ptr [rsp + 0x38], 4
0x140c1d2f8: mov dword ptr [rsp + 0x40], 0
0x140c1d300: mov qword ptr [rsp + 0x48], 0
0x140c1d309: lea rdx, [rip + 0x4a43dff]
0x140c1d310: jmp 0x140c1d449
0x140c1d315: mov byte ptr [rsp + 0x38], 0
0x140c1d31a: mov rdi, qword ptr [rsi + 0x7da0]
0x140c1d321: test rdi, rdi
0x140c1d324: je 0x140c1d37f
0x140c1d326: cmp rdi, qword ptr [rip + 0x54394e3]
0x140c1d32d: je 0x140c1d37f
0x140c1d32f: test byte ptr [rdi + 0x26], 1
0x140c1d333: jne 0x140c1d37f
0x140c1d335: mov rax, qword ptr [rdi]
0x140c1d338: mov rcx, rdi
0x140c1d33b: call qword ptr [rax + 0x20]
0x140c1d33e: test al, al
0x140c1d340: je 0x140c1d37f
0x140c1d342: mov rcx, qword ptr [rdi + 0x58]
0x140c1d346: mov rax, qword ptr [rcx]
0x140c1d349: call qword ptr [rax + 8]
0x140c1d34c: mov rdx, qword ptr [rax]
0x140c1d34f: mov rcx, rax
0x140c1d352: call qword ptr [rdx + 0x10]
0x140c1d355: mov ebx, eax
0x140c1d357: mov rax, qword ptr [rip + 0x550ea62]
0x140c1d35e: lea rcx, [rip + 0x550ea5b]
0x140c1d365: call qword ptr [rax + 0x10]
0x140c1d368: cmp ebx, eax
0x140c1d36a: jne 0x140c1d37f
0x140c1d36c: lea rdx, [rsp + 0x38]
0x140c1d371: mov rcx, rdi
0x140c1d374: xor r8d, r8d
0x140c1d377: xor r9d, r9d
0x140c1d37a: call 0x140973e70
0x140c1d37f: mov byte ptr [rsp + 0x38], 0
0x140c1d384: mov rdi, qword ptr [rsi + 0x7dc0]
0x140c1d38b: test rdi, rdi
0x140c1d38e: je 0x140c1d3e9
0x140c1d390: cmp rdi, qword ptr [rip + 0x5439479]
0x140c1d397: je 0x140c1d3e9
0x140c1d399: test byte ptr [rdi + 0x26], 1
0x140c1d39d: jne 0x140c1d3e9
0x140c1d39f: mov rax, qword ptr [rdi]
0x140c1d3a2: mov rcx, rdi
0x140c1d3a5: call qword ptr [rax + 0x20]
0x140c1d3a8: test al, al
0x140c1d3aa: je 0x140c1d3e9
0x140c1d3ac: mov rcx, qword ptr [rdi + 0x58]
0x140c1d3b0: mov rax, qword ptr [rcx]
0x140c1d3b3: call qword ptr [rax + 8]
0x140c1d3b6: mov rdx, qword ptr [rax]
0x140c1d3b9: mov rcx, rax
0x140c1d3bc: call qword ptr [rdx + 0x10]
0x140c1d3bf: mov ebx, eax
0x140c1d3c1: mov rax, qword ptr [rip + 0x550e9f8]
0x140c1d3c8: lea rcx, [rip + 0x550e9f1]
0x140c1d3cf: call qword ptr [rax + 0x10]
0x140c1d3d2: cmp ebx, eax
0x140c1d3d4: jne 0x140c1d3e9
0x140c1d3d6: lea rdx, [rsp + 0x38]
0x140c1d3db: mov rcx, rdi
0x140c1d3de: xor r8d, r8d
0x140c1d3e1: xor r9d, r9d
0x140c1d3e4: call 0x140973e70
0x140c1d3e9: lea rcx, [rsi + 0x7e18]
0x140c1d3f0: call 0x1442cf090
0x140c1d3f5: mov dword ptr [rsp + 0x38], 4
0x140c1d3fd: mov dword ptr [rsp + 0x40], 0
0x140c1d405: mov qword ptr [rsp + 0x48], 0
0x140c1d40e: lea rdx, [rip + 0x4a72b32]
0x140c1d415: jmp 0x140c1d449
0x140c1d417: mov rcx, rsi
0x140c1d41a: add rsp, 0xf0
0x140c1d421: pop rbx
0x140c1d422: pop rdi
0x140c1d423: pop rsi
0x140c1d424: jmp 0x140c1d4b0
0x140c1d429: mov dword ptr [rsp + 0x38], 4
0x140c1d431: mov dword ptr [rsp + 0x40], 0
0x140c1d439: mov qword ptr [rsp + 0x48], 0
0x140c1d442: lea rdx, [rip + 0x4a71b91]
0x140c1d449: lea r8, [rsp + 0x38]
0x140c1d44e: mov rcx, rsi
0x140c1d451: xor r9d, r9d
0x140c1d454: call 0x1443235b0
0x140c1d459: mov rax, qword ptr [rsp + 0x48]
0x140c1d45e: test rax, rax
0x140c1d461: je 0x140c1d497
0x140c1d463: lock dec word ptr [rax + 4]
0x140c1d468: mov rcx, qword ptr [rsp + 0x48]
0x140c1d46d: mov dx, 0xffff
0x140c1d471: xor eax, eax
0x140c1d473: lock cmpxchg word ptr [rcx + 4], dx
0x140c1d479: jne 0x140c1d497
0x140c1d47b: mov rcx, qword ptr [rip + 0x608a026]
0x140c1d482: test rcx, rcx
0x140c1d485: je 0x140c1d497
0x140c1d487: mov rdx, qword ptr [rsp + 0x48]
0x140c1d48c: test rdx, rdx
0x140c1d48f: je 0x140c1d497
0x140c1d491: mov rax, qword ptr [rcx]
0x140c1d494: call qword ptr [rax + 0x38]
0x140c1d497: nop
0x140c1d498: add rsp, 0xf0
0x140c1d49f: pop rbx
0x140c1d4a0: pop rdi
0x140c1d4a1: pop rsi
0x140c1d4a2: ret
