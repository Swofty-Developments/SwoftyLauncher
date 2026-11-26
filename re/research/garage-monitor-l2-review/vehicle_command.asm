0x141f59730: push r15
0x141f59732: push r14
0x141f59734: push r13
0x141f59736: push r12
0x141f59738: push rsi
0x141f59739: push rdi
0x141f5973a: push rbp
0x141f5973b: push rbx
0x141f5973c: sub rsp, 0x128
0x141f59743: cmp qword ptr [rcx + 0x650], 0
0x141f5974b: je 0x141f599df
0x141f59751: mov rdi, rdx
0x141f59754: mov rsi, rcx
0x141f59757: mov rcx, qword ptr [rcx + 0x620]
0x141f5975e: test rcx, rcx
0x141f59761: je 0x141f5976f
0x141f59763: mov rax, qword ptr [rcx]
0x141f59766: mov rdx, rdi
0x141f59769: call qword ptr [rax + 0x80]
0x141f5976f: mov rax, qword ptr [rdi]
0x141f59772: test rax, rax
0x141f59775: je 0x141f599df
0x141f5977b: mov eax, dword ptr [rax]
0x141f5977d: cmp eax, 0x28d6ae89
0x141f59782: jg 0x141f59852
0x141f59788: cmp eax, 0xec04e0b0
0x141f5978d: je 0x141f59959
0x141f59793: cmp eax, 0x1dce7a87
0x141f59798: jne 0x141f599df
0x141f5979e: mov rcx, qword ptr [rsi + 0x630]
0x141f597a5: xor edx, edx
0x141f597a7: call 0x1424ae040
0x141f597ac: test al, al
0x141f597ae: je 0x141f599f4
0x141f597b4: mov rcx, qword ptr [rsi + 0x648]
0x141f597bb: mov rax, qword ptr [rcx + 0x10]
0x141f597bf: add rcx, 0x10
0x141f597c3: call qword ptr [rax + 0x58]
0x141f597c6: lea rcx, [rip + 0xbd3]
0x141f597cd: mov qword ptr [rsp + 0x48], rcx
0x141f597d2: mov qword ptr [rsp + 0x50], rsi
0x141f597d7: lea rcx, [rsp + 0x70]
0x141f597dc: mov qword ptr [rsp + 0x60], 0x80
0x141f597e5: mov qword ptr [rsp + 0x68], rcx
0x141f597ea: vmovaps xmm0, xmmword ptr [rip + 0x371a5ce]
0x141f597f2: vmovups xmmword ptr [rsp + 0x70], xmm0
0x141f597f8: vmovaps xmm0, xmmword ptr [rip + 0x371a5d0]
0x141f59800: vmovups xmmword ptr [rsp + 0x80], xmm0
0x141f59809: mov word ptr [rsp + 0x90], 0x67
0x141f59813: lea rcx, [rip + 0x36bbe56]
0x141f5981a: mov qword ptr [rsp + 0x58], rcx
0x141f5981f: vxorps xmm0, xmm0, xmm0
0x141f59823: vmovups xmmword ptr [rsp + 0x20], xmm0
0x141f59829: lea rdi, [rsp + 0xf0]
0x141f59831: lea r8, [rsp + 0x58]
0x141f59836: lea r9, [rsp + 0x48]
0x141f5983b: mov rcx, rax
0x141f5983e: mov rdx, rdi
0x141f59841: call 0x1442dc1c0
0x141f59846: add rsi, 0x12c0
0x141f5984d: jmp 0x141f599cc
0x141f59852: cmp eax, 0x28d6ae8a
0x141f59857: je 0x141f5997c
0x141f5985d: cmp eax, 0x461d9dde
0x141f59862: jne 0x141f599df
0x141f59868: mov rax, qword ptr [rsi + 0x650]
0x141f5986f: mov rax, qword ptr [rax + 0x248]
0x141f59876: lea rdi, [rax + 8]
0x141f5987a: test rax, rax
0x141f5987d: lea rbx, [rip + 0x4d4960c]
0x141f59884: cmove rdi, rbx
0x141f59888: mov rcx, qword ptr [rdi]
0x141f5988b: test rcx, rcx
0x141f5988e: je 0x141f598fa
0x141f59890: mov rax, qword ptr [rcx]
0x141f59893: call qword ptr [rax + 0x30]
0x141f59896: mov r14, qword ptr [rdi]
0x141f59899: test al, al
0x141f5989b: je 0x141f598e8
0x141f5989d: test r14, r14
0x141f598a0: je 0x141f598e8
0x141f598a2: xor r14d, r14d
0x141f598a5: xor eax, eax
0x141f598a7: nop word ptr [rax + rax]
0x141f598b0: mov r15d, eax
0x141f598b3: mov rax, qword ptr [rdi]
0x141f598b6: mov rcx, qword ptr [rax + 0x58]
0x141f598ba: mov rax, qword ptr [rcx]
0x141f598bd: call qword ptr [rax + 0x38]
0x141f598c0: mov rcx, qword ptr [rax]
0x141f598c3: test rcx, rcx
0x141f598c6: je 0x141f598e2
0x141f598c8: mov rdi, rax
0x141f598cb: mov rax, qword ptr [rcx]
0x141f598ce: call qword ptr [rax + 0x30]
0x141f598d1: test al, al
0x141f598d3: je 0x141f598df
0x141f598d5: lea eax, [r15 + 1]
0x141f598d9: cmp r15d, 0x62
0x141f598dd: jb 0x141f598b0
0x141f598df: mov r14, qword ptr [rdi]
0x141f598e2: cmp r15d, 0x61
0x141f598e6: ja 0x141f598fa
0x141f598e8: lea rax, [r14 + 8]
0x141f598ec: test r14, r14
0x141f598ef: lea rbx, [rip + 0x4d4959a]
0x141f598f6: cmovne rbx, rax
0x141f598fa: mov rax, qword ptr [rsi + 0x650]
0x141f59901: mov rdi, qword ptr [rax + 0x228]
0x141f59908: mov rax, qword ptr [rax + 0x230]
0x141f5990f: cmp rdi, rax
0x141f59912: je 0x141f599df
0x141f59918: mov rcx, qword ptr [rbx]
0x141f5991b: lea r15, [rip + 0x4d4956e]
0x141f59922: nop word ptr cs:[rax + rax]
0x141f59930: mov rdx, qword ptr [rdi + 0x10]
0x141f59934: lea r8, [rdx + 8]
0x141f59938: test rdx, rdx
0x141f5993b: cmove r8, r15
0x141f5993f: cmp qword ptr [r8], rcx
0x141f59942: je 0x141f59a19
0x141f59948: add rdi, 0x2c8
0x141f5994f: cmp rdi, rax
0x141f59952: jne 0x141f59930
0x141f59954: jmp 0x141f599df
0x141f59959: mov rcx, rsi
0x141f5995c: call 0x141f56f00
0x141f59961: mov rcx, rsi
0x141f59964: add rsp, 0x128
0x141f5996b: pop rbx
0x141f5996c: pop rbp
0x141f5996d: pop rdi
0x141f5996e: pop rsi
0x141f5996f: pop r12
0x141f59971: pop r13
0x141f59973: pop r14
0x141f59975: pop r15
0x141f59977: jmp 0x141f57c90
0x141f5997c: mov rcx, qword ptr [rsi + 0x658]
0x141f59983: vxorps xmm0, xmm0, xmm0
0x141f59987: vmovaps xmmword ptr [rsp + 0x110], xmm0
0x141f59990: lea rax, [rsp + 0x110]
0x141f59998: mov qword ptr [rsp + 0x20], rax
0x141f5999d: mov byte ptr [rsp + 0x38], 1
0x141f599a2: mov byte ptr [rsp + 0x30], 0
0x141f599a7: mov byte ptr [rsp + 0x28], 0
0x141f599ac: lea rdi, [rsp + 0x58]
0x141f599b1: mov rdx, rdi
0x141f599b4: mov r8d, 0x63
0x141f599ba: mov r9d, 1
0x141f599c0: call 0x142e750d0
0x141f599c5: add rsi, 0x12d0
0x141f599cc: mov rcx, rsi
0x141f599cf: mov rdx, rdi
0x141f599d2: call 0x1442acc90
0x141f599d7: mov rcx, rdi
0x141f599da: call 0x1442acf50
0x141f599df: nop
0x141f599e0: add rsp, 0x128
0x141f599e7: pop rbx
0x141f599e8: pop rbp
0x141f599e9: pop rdi
0x141f599ea: pop rsi
0x141f599eb: pop r12
0x141f599ed: pop r13
0x141f599ef: pop r14
0x141f599f1: pop r15
0x141f599f3: ret
0x141f599f4: mov dword ptr [rsp + 0x58], 4
0x141f599fc: mov dword ptr [rsp + 0x60], 0
0x141f59a04: mov qword ptr [rsp + 0x68], 0
0x141f59a0d: lea rdx, [rip + 0x37355c6]
0x141f59a14: jmp 0x141f59de3
0x141f59a19: mov rax, qword ptr [rdi + 0x1d0]
0x141f59a20: lea rbx, [rax + 8]
0x141f59a24: test rax, rax
0x141f59a27: cmove rbx, r15
0x141f59a2b: mov rcx, qword ptr [rbx]
0x141f59a2e: test rcx, rcx
0x141f59a31: je 0x141f59a95
0x141f59a33: mov rax, qword ptr [rcx]
0x141f59a36: call qword ptr [rax + 0x30]
0x141f59a39: mov r14, qword ptr [rbx]
0x141f59a3c: test al, al
0x141f59a3e: je 0x141f59a83
0x141f59a40: test r14, r14
0x141f59a43: je 0x141f59a83
0x141f59a45: xor r14d, r14d
0x141f59a48: xor eax, eax
0x141f59a4a: mov r12d, eax
0x141f59a4d: mov rax, qword ptr [rbx]
0x141f59a50: mov rcx, qword ptr [rax + 0x58]
0x141f59a54: mov rax, qword ptr [rcx]
0x141f59a57: call qword ptr [rax + 0x38]
0x141f59a5a: mov rcx, qword ptr [rax]
0x141f59a5d: test rcx, rcx
0x141f59a60: je 0x141f59a7d
0x141f59a62: mov rbx, rax
0x141f59a65: mov rax, qword ptr [rcx]
0x141f59a68: call qword ptr [rax + 0x30]
0x141f59a6b: test al, al
0x141f59a6d: je 0x141f59a7a
0x141f59a6f: lea eax, [r12 + 1]
0x141f59a74: cmp r12d, 0x62
0x141f59a78: jb 0x141f59a4a
0x141f59a7a: mov r14, qword ptr [rbx]
0x141f59a7d: cmp r12d, 0x61
0x141f59a81: ja 0x141f59a95
0x141f59a83: lea rax, [r14 + 8]
0x141f59a87: test r14, r14
0x141f59a8a: lea r15, [rip + 0x4d493ff]
0x141f59a91: cmovne r15, rax
0x141f59a95: mov r14, qword ptr [rdi + 0x170]
0x141f59a9c: mov rax, qword ptr [rdi + 0x178]
0x141f59aa3: cmp r14, rax
0x141f59aa6: je 0x141f59ae0
0x141f59aa8: mov rcx, qword ptr [r15]
0x141f59aab: lea rdx, [rip + 0x4d493de]
0x141f59ab2: nop word ptr cs:[rax + rax]
0x141f59ac0: mov r8, qword ptr [r14 + 0x10]
0x141f59ac4: lea r9, [r8 + 8]
0x141f59ac8: test r8, r8
0x141f59acb: cmove r9, rdx
0x141f59acf: cmp qword ptr [r9], rcx
0x141f59ad2: je 0x141f59ae3
0x141f59ad4: add r14, 0x1c8
0x141f59adb: cmp r14, rax
0x141f59ade: jne 0x141f59ac0
0x141f59ae0: xor r14d, r14d
0x141f59ae3: mov rax, qword ptr [rdi + 0x1b0]
0x141f59aea: lea rbx, [rax + 8]
0x141f59aee: test rax, rax
0x141f59af1: lea r13, [rip + 0x4d49398]
0x141f59af8: cmove rbx, r13
0x141f59afc: mov rcx, qword ptr [rbx]
0x141f59aff: test rcx, rcx
0x141f59b02: je 0x141f59b66
0x141f59b04: mov rax, qword ptr [rcx]
0x141f59b07: call qword ptr [rax + 0x30]
0x141f59b0a: mov r15, qword ptr [rbx]
0x141f59b0d: test al, al
0x141f59b0f: je 0x141f59b54
0x141f59b11: test r15, r15
0x141f59b14: je 0x141f59b54
0x141f59b16: xor r15d, r15d
0x141f59b19: xor eax, eax
0x141f59b1b: mov r12d, eax
0x141f59b1e: mov rax, qword ptr [rbx]
0x141f59b21: mov rcx, qword ptr [rax + 0x58]
0x141f59b25: mov rax, qword ptr [rcx]
0x141f59b28: call qword ptr [rax + 0x38]
0x141f59b2b: mov rcx, qword ptr [rax]
0x141f59b2e: test rcx, rcx
0x141f59b31: je 0x141f59b4e
0x141f59b33: mov rbx, rax
0x141f59b36: mov rax, qword ptr [rcx]
0x141f59b39: call qword ptr [rax + 0x30]
0x141f59b3c: test al, al
0x141f59b3e: je 0x141f59b4b
0x141f59b40: lea eax, [r12 + 1]
0x141f59b45: cmp r12d, 0x62
0x141f59b49: jb 0x141f59b1b
0x141f59b4b: mov r15, qword ptr [rbx]
0x141f59b4e: cmp r12d, 0x61
0x141f59b52: ja 0x141f59b66
0x141f59b54: lea rax, [r15 + 8]
0x141f59b58: test r15, r15
0x141f59b5b: lea r13, [rip + 0x4d4932e]
0x141f59b62: cmovne r13, rax
0x141f59b66: mov r15, qword ptr [rdi + 0x170]
0x141f59b6d: mov r12, qword ptr [rdi + 0x178]
0x141f59b74: cmp r15, r12
0x141f59b77: je 0x141f59ce0
0x141f59b7d: mov rcx, qword ptr [r13]
0x141f59b81: lea rdx, [rip + 0x4d49308]
0x141f59b88: mov rax, r15
0x141f59b8b: nop dword ptr [rax + rax]
0x141f59b90: mov r8, qword ptr [rax + 0x10]
0x141f59b94: lea r9, [r8 + 8]
0x141f59b98: test r8, r8
0x141f59b9b: cmove r9, rdx
0x141f59b9f: cmp qword ptr [r9], rcx
0x141f59ba2: je 0x141f59bb1
0x141f59ba4: add rax, 0x1c8
0x141f59baa: cmp rax, r12
0x141f59bad: jne 0x141f59b90
0x141f59baf: xor eax, eax
0x141f59bb1: test r14, r14
0x141f59bb4: je 0x141f59be0
0x141f59bb6: cmp r14, rax
0x141f59bb9: jne 0x141f59ce9
0x141f59bbf: jmp 0x141f59be0
0x141f59bc1: nop word ptr cs:[rax + rax]
0x141f59bd0: add r15, 0x1c8
0x141f59bd7: cmp r15, r12
0x141f59bda: je 0x141f59db6
0x141f59be0: mov rbx, qword ptr [r15 + 0x1b0]
0x141f59be7: mov rax, qword ptr [rip + 0x41132a2]
0x141f59bee: test rbx, rbx
0x141f59bf1: je 0x141f59c68
0x141f59bf3: cmp rbx, rax
0x141f59bf6: je 0x141f59c68
0x141f59bf8: mov rax, qword ptr [rbx]
0x141f59bfb: mov rcx, rbx
0x141f59bfe: call qword ptr [rax + 0x20]
0x141f59c01: test al, al
0x141f59c03: je 0x141f59c5a
0x141f59c05: mov rcx, qword ptr [rbx + 0x58]
0x141f59c09: mov rax, qword ptr [rcx]
0x141f59c0c: call qword ptr [rax + 8]
0x141f59c0f: mov rdx, qword ptr [rax]
0x141f59c12: mov rcx, rax
0x141f59c15: call qword ptr [rdx + 0x10]
0x141f59c18: mov ebp, eax
0x141f59c1a: mov rax, qword ptr [rip + 0x41d219f]
0x141f59c21: lea rcx, [rip + 0x41d2198]
0x141f59c28: call qword ptr [rax + 0x10]
0x141f59c2b: cmp ebp, eax
0x141f59c2d: jne 0x141f59c5a
0x141f59c2f: mov rcx, qword ptr [rbx + 0x58]
0x141f59c33: mov rax, qword ptr [rcx]
0x141f59c36: call qword ptr [rax + 0x38]
0x141f59c39: cmp byte ptr [rax], 0
0x141f59c3c: je 0x141f59c5a
0x141f59c3e: movsxd rax, dword ptr [rdi]
0x141f59c41: mov rcx, qword ptr [rsi + 0x698]
0x141f59c48: shl rax, 4
0x141f59c4c: mov rax, qword ptr [rcx + rax + 0x18]
0x141f59c51: vxorps xmm0, xmm0, xmm0
0x141f59c55: vmovups xmmword ptr [rax + 0x20], xmm0
0x141f59c5a: mov rbx, qword ptr [r15 + 0x1b0]
0x141f59c61: mov rax, qword ptr [rip + 0x4113228]
0x141f59c68: mov byte ptr [rsp + 0x58], 0
0x141f59c6d: test rbx, rbx
0x141f59c70: je 0x141f59bd0
0x141f59c76: cmp rbx, rax
0x141f59c79: je 0x141f59bd0
0x141f59c7f: test byte ptr [rbx + 0x26], 1
0x141f59c83: jne 0x141f59bd0
0x141f59c89: mov rax, qword ptr [rbx]
0x141f59c8c: mov rcx, rbx
0x141f59c8f: call qword ptr [rax + 0x20]
0x141f59c92: test al, al
0x141f59c94: je 0x141f59bd0
0x141f59c9a: mov rcx, qword ptr [rbx + 0x58]
0x141f59c9e: mov rax, qword ptr [rcx]
0x141f59ca1: call qword ptr [rax + 8]
0x141f59ca4: mov rdx, qword ptr [rax]
0x141f59ca7: mov rcx, rax
0x141f59caa: call qword ptr [rdx + 0x10]
0x141f59cad: mov ebp, eax
0x141f59caf: mov rax, qword ptr [rip + 0x41d210a]
0x141f59cb6: lea rcx, [rip + 0x41d2103]
0x141f59cbd: call qword ptr [rax + 0x10]
0x141f59cc0: cmp ebp, eax
0x141f59cc2: jne 0x141f59bd0
0x141f59cc8: lea rdx, [rsp + 0x58]
0x141f59ccd: mov rcx, rbx
0x141f59cd0: xor r8d, r8d
0x141f59cd3: xor r9d, r9d
0x141f59cd6: call 0x140973e70
0x141f59cdb: jmp 0x141f59bd0
0x141f59ce0: test r14, r14
0x141f59ce3: je 0x141f59db6
0x141f59ce9: mov rax, qword ptr [rip + 0x40ecc98]
0x141f59cf0: mov rax, qword ptr [rax + 0x20]
0x141f59cf4: mov rax, qword ptr [rax + 0x1a8]
0x141f59cfb: mov eax, dword ptr [rax + 4]
0x141f59cfe: cmp eax, 9
0x141f59d01: sete cl
0x141f59d04: cmp eax, 0xe
0x141f59d07: sete r15b
0x141f59d0b: or r15b, cl
0x141f59d0e: cmp byte ptr [rsi + 0x12b8], 0
0x141f59d15: sete r9b
0x141f59d19: or r15b, r9b
0x141f59d1c: mov rbx, qword ptr [rsi + 0x690]
0x141f59d23: mov rdx, qword ptr [rsi + 0x698]
0x141f59d2a: mov ebp, dword ptr [rdi]
0x141f59d2c: mov rcx, rbx
0x141f59d2f: mov r8d, ebp
0x141f59d32: call 0x1411d1670
0x141f59d37: test al, al
0x141f59d39: je 0x141f59e37
0x141f59d3f: mov rcx, qword ptr [rbx + 0x278]
0x141f59d46: mov rax, qword ptr [rcx]
0x141f59d49: lea rdx, [rsp + 0x58]
0x141f59d4e: call qword ptr [rax + 0xc8]
0x141f59d54: movzx edx, word ptr [rsp + 0x5a]
0x141f59d59: cmp edx, 0xff
0x141f59d5f: jbe 0x141f5a38c
0x141f59d65: movsxd rax, ebp
0x141f59d68: mov rcx, qword ptr [rbx + 0x340]
0x141f59d6f: mov rcx, qword ptr [rcx + 0x50]
0x141f59d73: cmp byte ptr [rcx], dl
0x141f59d75: je 0x141f59d94
0x141f59d77: add rcx, 0x250
0x141f59d7e: mov r8, rcx
0x141f59d81: add rcx, 0x250
0x141f59d88: cmp byte ptr [r8], dl
0x141f59d8b: jne 0x141f59d7e
0x141f59d8d: add rcx, -0x250
0x141f59d94: imul rax, rax, 0x54
0x141f59d98: cmp dword ptr [rcx + rax + 0x24], 0
0x141f59d9d: je 0x141f59e37
0x141f59da3: lea rax, [rip + 0x380759c]
0x141f59daa: lea rbx, [rip + 0x380756e]
0x141f59db1: jmp 0x141f59e8d
0x141f59db6: mov rcx, rdi
0x141f59db9: xor edx, edx
0x141f59dbb: xor r8d, r8d
0x141f59dbe: call 0x141f59440
0x141f59dc3: mov dword ptr [rsp + 0x58], 4
0x141f59dcb: mov dword ptr [rsp + 0x60], 0
0x141f59dd3: mov qword ptr [rsp + 0x68], 0
0x141f59ddc: lea rdx, [rip + 0x3807631]
0x141f59de3: lea r8, [rsp + 0x58]
0x141f59de8: mov rcx, rsi
0x141f59deb: xor r9d, r9d
0x141f59dee: call 0x1443235b0
0x141f59df3: mov rax, qword ptr [rsp + 0x68]
0x141f59df8: test rax, rax
0x141f59dfb: je 0x141f599df
0x141f59e01: lock dec word ptr [rax + 4]
0x141f59e06: mov rcx, qword ptr [rsp + 0x68]
0x141f59e0b: mov dx, 0xffff
0x141f59e0f: xor eax, eax
0x141f59e11: lock cmpxchg word ptr [rcx + 4], dx
0x141f59e17: jne 0x141f599df
0x141f59e1d: mov rcx, qword ptr [rip + 0x4d4d684]
0x141f59e24: test rcx, rcx
0x141f59e27: je 0x141f599df
0x141f59e2d: mov rdx, qword ptr [rsp + 0x68]
0x141f59e32: jmp 0x141f5a378
0x141f59e37: mov rdx, qword ptr [rsi + 0x698]
0x141f59e3e: movsxd r8, dword ptr [rdi]
0x141f59e41: mov rax, r8
0x141f59e44: shl rax, 4
0x141f59e48: mov rax, qword ptr [rdx + rax + 0x18]
0x141f59e4d: test rax, rax
0x141f59e50: je 0x141f59e73
0x141f59e52: mov rcx, qword ptr [rax + 0x10]
0x141f59e56: sub rcx, qword ptr [rax + 8]
0x141f59e5a: sar rcx, 4
0x141f59e5e: cmp qword ptr [r14], rcx
0x141f59e61: jae 0x141f59e73
0x141f59e63: lea rax, [rip + 0x380756b]
0x141f59e6a: lea rbx, [rip + 0x3807540]
0x141f59e71: jmp 0x141f59e8d
0x141f59e73: mov rcx, qword ptr [rsi + 0x690]
0x141f59e7a: call 0x1411d0cc0
0x141f59e7f: lea rax, [rip + 0x3807509]
0x141f59e86: lea rbx, [rip + 0x38074e4]
0x141f59e8d: test r15b, r15b
0x141f59e90: cmovne rbx, rax
0x141f59e94: mov rax, qword ptr [rsi + 0x640]
0x141f59e9b: mov rax, qword ptr [rax + 0x38]
0x141f59e9f: test rax, rax
0x141f59ea2: je 0x141f59ecd
0x141f59ea4: mov ecx, dword ptr [rdi]
0x141f59ea6: lea rdx, [rcx + 9]
0x141f59eaa: cmp rcx, 7
0x141f59eae: mov ecx, 0x15
0x141f59eb3: cmovb rcx, rdx
0x141f59eb7: lea rcx, [rcx + rcx*2]
0x141f59ebb: vmovss xmm1, dword ptr [rax]
0x141f59ebf: vmulss xmm0, xmm1, dword ptr [rax + rcx*4 + 0x18]
0x141f59ec5: vmulss xmm1, xmm1, dword ptr [rax + rcx*4 + 0x1c]
0x141f59ecb: jmp 0x141f59ed5
0x141f59ecd: vxorps xmm0, xmm0, xmm0
0x141f59ed1: vxorps xmm1, xmm1, xmm1
0x141f59ed5: vmovss xmm2, dword ptr [rip + 0x3429777]
0x141f59edd: vmulss xmm0, xmm0, xmm2
0x141f59ee1: vcvttss2si r14, xmm0
0x141f59ee6: vmulss xmm0, xmm1, xmm2
0x141f59eea: vcvttss2si rdi, xmm0
0x141f59eef: mov qword ptr [rsp + 0xf0], 7
0x141f59efb: mov dword ptr [rsp + 0xf8], 0
0x141f59f06: mov word ptr [rsp + 0xfc], 1
0x141f59f10: mov byte ptr [rsp + 0xfe], 0
0x141f59f18: mov rcx, qword ptr [rsi + 0x648]
0x141f59f1f: mov rax, qword ptr [rcx + 0x10]
0x141f59f23: add rcx, 0x10
0x141f59f27: call qword ptr [rax + 0x58]
0x141f59f2a: lea rcx, [rip + 0x6ef]
0x141f59f31: mov qword ptr [rsp + 0x100], rcx
0x141f59f39: mov qword ptr [rsp + 0x108], rsi
0x141f59f41: lea rcx, [rsp + 0x70]
0x141f59f46: mov qword ptr [rsp + 0x60], 0x80
0x141f59f4f: mov qword ptr [rsp + 0x68], rcx
0x141f59f54: xor edx, edx
0x141f59f56: movzx r8d, byte ptr [rbx + rdx]
0x141f59f5b: test r8b, r8b
0x141f59f5e: je 0x141f59fa5
0x141f59f60: mov byte ptr [rsp + rdx + 0x70], r8b
0x141f59f65: movzx r8d, byte ptr [rbx + rdx + 1]
0x141f59f6b: test r8b, r8b
0x141f59f6e: je 0x141f59faf
0x141f59f70: mov byte ptr [rsp + rdx + 0x71], r8b
0x141f59f75: movzx r8d, byte ptr [rbx + rdx + 2]
0x141f59f7b: test r8b, r8b
0x141f59f7e: je 0x141f59fb9
0x141f59f80: mov byte ptr [rsp + rdx + 0x72], r8b
0x141f59f85: cmp rdx, 0x7c
0x141f59f89: je 0x141f59fc3
0x141f59f8b: movzx r8d, byte ptr [rbx + rdx + 3]
0x141f59f91: test r8b, r8b
0x141f59f94: je 0x141f59fcd
0x141f59f96: add rcx, 4
0x141f59f9a: mov byte ptr [rsp + rdx + 0x73], r8b
0x141f59f9f: add rdx, 4
0x141f59fa3: jmp 0x141f59f56
0x141f59fa5: lea rcx, [rsp + rdx]
0x141f59fa9: add rcx, 0x70
0x141f59fad: jmp 0x141f59fd1
0x141f59faf: lea rcx, [rsp + rdx]
0x141f59fb3: add rcx, 0x71
0x141f59fb7: jmp 0x141f59fd1
0x141f59fb9: lea rcx, [rsp + rdx]
0x141f59fbd: add rcx, 0x72
0x141f59fc1: jmp 0x141f59fd1
0x141f59fc3: lea rcx, [rsp + rdx]
0x141f59fc7: add rcx, 0x73
0x141f59fcb: jmp 0x141f59fd1
0x141f59fcd: add rcx, 3
0x141f59fd1: mov byte ptr [rcx], 0
0x141f59fd4: lea rcx, [rip + 0x36bb695]
0x141f59fdb: mov qword ptr [rsp + 0x58], rcx
0x141f59fe0: vxorps xmm0, xmm0, xmm0
0x141f59fe4: vmovups xmmword ptr [rsp + 0x20], xmm0
0x141f59fea: lea rbx, [rsp + 0x48]
0x141f59fef: lea r8, [rsp + 0x58]
0x141f59ff4: lea r9, [rsp + 0x100]
0x141f59ffc: mov rcx, rax
0x141f59fff: mov rdx, rbx
0x141f5a002: call 0x1442dc1c0
0x141f5a007: lea rcx, [rsi + 0x680]
0x141f5a00e: mov rdx, rbx
0x141f5a011: call 0x1442acc90
0x141f5a016: mov rcx, rbx
0x141f5a019: call 0x1442acf50
0x141f5a01e: mov rcx, qword ptr [rip + 0x4d4d4d3]
0x141f5a025: mov rax, qword ptr [rcx + 8]
0x141f5a029: lea rdx, [rax + 8]
0x141f5a02d: test rax, rax
0x141f5a030: lea rax, [rip + 0x4d48e59]
0x141f5a037: cmove rdx, rax
0x141f5a03b: lea r8, [rip + 0x38073bb]
0x141f5a042: movabs r9, 0x500000000000801
0x141f5a04c: movzx r10d, byte ptr [r8]
0x141f5a050: lea r11d, [r10 - 0x23]
0x141f5a054: cmp r11d, 0x3a
0x141f5a058: ja 0x141f5a060
0x141f5a05a: bt r9, r11
0x141f5a05e: jb 0x141f5a07c
0x141f5a060: test r10d, r10d
0x141f5a063: je 0x141f5a06a
0x141f5a065: inc r8
0x141f5a068: jmp 0x141f5a04c
0x141f5a06a: mov dword ptr [rsp + 0x58], 0xad5c8fd7
0x141f5a072: lea r8, [rsp + 0x58]
0x141f5a077: call 0x1442b51c0
0x141f5a07c: lea r12, [rip + 0x36badc5]
0x141f5a083: mov qword ptr [rsp + 0x58], r12
0x141f5a088: lea rbx, [rsp + 0x60]
0x141f5a08d: vxorps xmm0, xmm0, xmm0
0x141f5a091: vmovups xmmword ptr [rsp + 0x60], xmm0
0x141f5a097: mov qword ptr [rsp + 0x70], 0
0x141f5a0a0: mov rax, qword ptr [rax]
0x141f5a0a3: mov qword ptr [rsp + 0x60], rax
0x141f5a0a8: test rax, rax
0x141f5a0ab: je 0x141f5a0d1
0x141f5a0ad: mov rcx, qword ptr [rax + 0x28]
0x141f5a0b1: mov qword ptr [rsp + 0x68], rcx
0x141f5a0b6: mov qword ptr [rsp + 0x70], 0
0x141f5a0bf: test rcx, rcx
0x141f5a0c2: je 0x141f5a0c8
0x141f5a0c4: mov qword ptr [rcx + 0x10], rbx
0x141f5a0c8: mov qword ptr [rax + 0x28], rbx
0x141f5a0cc: lock inc word ptr [rax + 0x24]
0x141f5a0d1: lea rax, [rip + 0x3707300]
0x141f5a0d8: mov qword ptr [rsp + 0x58], rax
0x141f5a0dd: add rsi, 0x668
0x141f5a0e4: mov rcx, rsi
0x141f5a0e7: mov rdx, rbx
0x141f5a0ea: call 0x1442be740
0x141f5a0ef: mov qword ptr [rsp + 0x58], r12
0x141f5a0f4: mov rax, qword ptr [rsp + 0x60]
0x141f5a0f9: test rax, rax
0x141f5a0fc: je 0x141f5a143
0x141f5a0fe: mov rcx, qword ptr [rsp + 0x68]
0x141f5a103: mov rdx, qword ptr [rsp + 0x70]
0x141f5a108: test rdx, rdx
0x141f5a10b: je 0x141f5a111
0x141f5a10d: mov qword ptr [rdx + 8], rcx
0x141f5a111: lea r8, [rsp + 0x68]
0x141f5a116: test rcx, rcx
0x141f5a119: je 0x141f5a11f
0x141f5a11b: mov qword ptr [rcx + 0x10], rdx
0x141f5a11f: vxorps xmm0, xmm0, xmm0
0x141f5a123: vmovups xmmword ptr [r8], xmm0
0x141f5a128: cmp qword ptr [rax + 0x28], rbx
0x141f5a12c: jne 0x141f5a132
0x141f5a12e: mov qword ptr [rax + 0x28], rcx
0x141f5a132: mov rcx, qword ptr [rip + 0x4d4d3bf]
0x141f5a139: mov rdx, qword ptr [rsp + 0x60]
0x141f5a13e: call 0x1442b6340
0x141f5a143: lea rcx, [rsp + 0xf0]
0x141f5a14b: lea r15, [rsp + 0x48]
0x141f5a150: mov rdx, r15
0x141f5a153: mov r8, r14
0x141f5a156: xor r9d, r9d
0x141f5a159: call 0x1441f15f0
0x141f5a15e: mov rax, qword ptr [rsi]
0x141f5a161: lea rdx, [rax + 8]
0x141f5a165: test rax, rax
0x141f5a168: lea r13, [rip + 0x4d48d21]
0x141f5a16f: cmove rdx, r13
0x141f5a173: mov rcx, qword ptr [rip + 0x4d4d37e]
0x141f5a17a: mov dword ptr [rsp + 0x20], 1
0x141f5a182: lea r8, [rip + 0x3706f00]
0x141f5a189: mov r9, r15
0x141f5a18c: call 0x1408462c0
0x141f5a191: mov qword ptr [rsp + 0x58], r12
0x141f5a196: vxorps xmm0, xmm0, xmm0
0x141f5a19a: vmovups xmmword ptr [rsp + 0x60], xmm0
0x141f5a1a0: mov qword ptr [rsp + 0x70], 0
0x141f5a1a9: mov rax, qword ptr [rax]
0x141f5a1ac: mov qword ptr [rsp + 0x60], rax
0x141f5a1b1: test rax, rax
0x141f5a1b4: je 0x141f5a22a
0x141f5a1b6: mov rcx, qword ptr [rax + 0x28]
0x141f5a1ba: mov qword ptr [rsp + 0x68], rcx
0x141f5a1bf: mov qword ptr [rsp + 0x70], 0
0x141f5a1c8: test rcx, rcx
0x141f5a1cb: je 0x141f5a1d1
0x141f5a1cd: mov qword ptr [rcx + 0x10], rbx
0x141f5a1d1: mov qword ptr [rax + 0x28], rbx
0x141f5a1d5: lock inc word ptr [rax + 0x24]
0x141f5a1da: mov rax, qword ptr [rsp + 0x60]
0x141f5a1df: mov qword ptr [rsp + 0x58], r12
0x141f5a1e4: test rax, rax
0x141f5a1e7: je 0x141f5a22a
0x141f5a1e9: mov rcx, qword ptr [rsp + 0x68]
0x141f5a1ee: mov rdx, qword ptr [rsp + 0x70]
0x141f5a1f3: test rdx, rdx
0x141f5a1f6: je 0x141f5a1fc
0x141f5a1f8: mov qword ptr [rdx + 8], rcx
0x141f5a1fc: lea r8, [rsp + 0x68]
0x141f5a201: test rcx, rcx
0x141f5a204: je 0x141f5a20a
0x141f5a206: mov qword ptr [rcx + 0x10], rdx
0x141f5a20a: vmovups xmmword ptr [r8], xmm0
0x141f5a20f: cmp qword ptr [rax + 0x28], rbx
0x141f5a213: jne 0x141f5a219
0x141f5a215: mov qword ptr [rax + 0x28], rcx
0x141f5a219: mov rcx, qword ptr [rip + 0x4d4d2d8]
0x141f5a220: mov rdx, qword ptr [rsp + 0x60]
0x141f5a225: call 0x1442b6340
0x141f5a22a: mov rax, qword ptr [rsp + 0x48]
0x141f5a22f: test rax, rax
0x141f5a232: je 0x141f5a268
0x141f5a234: lock dec word ptr [rax + 4]
0x141f5a239: mov rcx, qword ptr [rsp + 0x48]
0x141f5a23e: mov dx, 0xffff
0x141f5a242: xor eax, eax
0x141f5a244: lock cmpxchg word ptr [rcx + 4], dx
0x141f5a24a: jne 0x141f5a268
0x141f5a24c: mov rcx, qword ptr [rip + 0x4d4d255]
0x141f5a253: test rcx, rcx
0x141f5a256: je 0x141f5a268
0x141f5a258: mov rdx, qword ptr [rsp + 0x48]
0x141f5a25d: test rdx, rdx
0x141f5a260: je 0x141f5a268
0x141f5a262: mov rax, qword ptr [rcx]
0x141f5a265: call qword ptr [rax + 0x38]
0x141f5a268: lea rcx, [rsp + 0xf0]
0x141f5a270: lea r14, [rsp + 0x48]
0x141f5a275: mov rdx, r14
0x141f5a278: mov r8, rdi
0x141f5a27b: xor r9d, r9d
0x141f5a27e: call 0x1441f15f0
0x141f5a283: mov rax, qword ptr [rsi]
0x141f5a286: lea rdx, [rax + 8]
0x141f5a28a: test rax, rax
0x141f5a28d: cmove rdx, r13
0x141f5a291: mov rcx, qword ptr [rip + 0x4d4d260]
0x141f5a298: mov dword ptr [rsp + 0x20], 1
0x141f5a2a0: lea r8, [rip + 0x36e0f4d]
0x141f5a2a7: mov r9, r14
0x141f5a2aa: call 0x1408462c0
0x141f5a2af: mov qword ptr [rsp + 0x58], r12
0x141f5a2b4: mov rax, qword ptr [rax]
0x141f5a2b7: mov qword ptr [rsp + 0x60], rax
0x141f5a2bc: test rax, rax
0x141f5a2bf: je 0x141f5a339
0x141f5a2c1: mov rcx, qword ptr [rax + 0x28]
0x141f5a2c5: mov qword ptr [rsp + 0x68], rcx
0x141f5a2ca: mov qword ptr [rsp + 0x70], 0
0x141f5a2d3: test rcx, rcx
0x141f5a2d6: je 0x141f5a2dc
0x141f5a2d8: mov qword ptr [rcx + 0x10], rbx
0x141f5a2dc: mov qword ptr [rax + 0x28], rbx
0x141f5a2e0: lock inc word ptr [rax + 0x24]
0x141f5a2e5: mov rax, qword ptr [rsp + 0x60]
0x141f5a2ea: mov qword ptr [rsp + 0x58], r12
0x141f5a2ef: test rax, rax
0x141f5a2f2: je 0x141f5a339
0x141f5a2f4: mov rcx, qword ptr [rsp + 0x68]
0x141f5a2f9: mov rdx, qword ptr [rsp + 0x70]
0x141f5a2fe: test rdx, rdx
0x141f5a301: je 0x141f5a307
0x141f5a303: mov qword ptr [rdx + 8], rcx
0x141f5a307: lea r8, [rsp + 0x68]
0x141f5a30c: test rcx, rcx
0x141f5a30f: je 0x141f5a315
0x141f5a311: mov qword ptr [rcx + 0x10], rdx
0x141f5a315: vxorps xmm0, xmm0, xmm0
0x141f5a319: vmovups xmmword ptr [r8], xmm0
0x141f5a31e: cmp qword ptr [rax + 0x28], rbx
0x141f5a322: jne 0x141f5a328
0x141f5a324: mov qword ptr [rax + 0x28], rcx
0x141f5a328: mov rcx, qword ptr [rip + 0x4d4d1c9]
0x141f5a32f: mov rdx, qword ptr [rsp + 0x60]
0x141f5a334: call 0x1442b6340
0x141f5a339: mov rax, qword ptr [rsp + 0x48]
0x141f5a33e: test rax, rax
0x141f5a341: je 0x141f599df
0x141f5a347: lock dec word ptr [rax + 4]
0x141f5a34c: mov rcx, qword ptr [rsp + 0x48]
0x141f5a351: mov dx, 0xffff
0x141f5a355: xor eax, eax
0x141f5a357: lock cmpxchg word ptr [rcx + 4], dx
0x141f5a35d: jne 0x141f599df
0x141f5a363: mov rcx, qword ptr [rip + 0x4d4d13e]
0x141f5a36a: test rcx, rcx
0x141f5a36d: je 0x141f599df
0x141f5a373: mov rdx, qword ptr [rsp + 0x48]
0x141f5a378: test rdx, rdx
0x141f5a37b: je 0x141f599df
0x141f5a381: mov rax, qword ptr [rcx]
0x141f5a384: call qword ptr [rax + 0x38]
0x141f5a387: jmp 0x141f599df
0x141f5a38c: call 0x14029b020
0x141f5a391: int3
