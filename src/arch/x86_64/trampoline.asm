[bits 16]

section .trampoline vstart=0x7500

global args
args:
    .safety    dq 0x54855fafb595ad
    .pml_addr  dq 0x56
    .stack     dq 0x72
    .target    dq 0x85
    .frarg     dq 0x95
    .cr4       dq 0x0
    .flags     db 0b10

times (0x8000 - 0x7500) - ($ - args) db 0

[bits 16]
global trampoline_fn

trampoline_fn:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax

    lgdt [gdtr_32]

    mov eax, cr0
    or eax, 1
    mov cr0, eax

    jmp 0x08:trampoline_32

[bits 32]
trampoline_32:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov fs, ax
    mov gs, ax

    xor ebx, ebx

    mov eax, cr4
    or eax, 1 << 5
    mov cr4, eax

    movzx ecx, byte [args.flags]

    test cl, 1
    jz .not_nx
    mov ecx, 0xC0000080
    rdmsr
    or eax, 1 << 11
    wrmsr
.not_nx:
    movzx ecx, byte [args.flags]
    test cl, 0b10
    jz .jmp_to_64
    mov eax, cr4
    or eax, 1 << 12
    mov cr4, eax

    mov ebx, 1

.jmp_to_64:
    test ebx, ebx
    jz p4
    mov eax, tmp_p5
    mov cr3, eax
    jmp ed
    p4:
        mov eax, tmp_p4
        mov cr3, eax
    ed:
    mov ecx, 0xC0000080
    rdmsr
    or eax, 1 << 8
    wrmsr

    mov eax, cr0
    or eax, 1 << 31
    mov cr0, eax

    lgdt [gdtr_64]
    jmp 0x18:trampoline_64

[bits 64]
trampoline_64:
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov fs, ax
    mov gs, ax

    mov rax, [args.cr4]
    mov cr4, rax

    mov rax, [args.pml_addr]
    mov cr3, rax

    lea rax, [rel .flushed]
    jmp rax
.flushed:
    mov rbx, [args.stack]
    sub rbx, 8
    xor rdx, rdx
    jmp .find_iter

; ---

.find_iter_ended:
    mov eax, 0x4ffff
    ud2
.find_iter:
    add rbx, 8
    mov rax, [rbx]

    test rax, rax
    jz .next
    cmp rax, 1
    je .find_iter_ended

    xor rcx, rcx
    lock cmpxchg [rbx], rcx

    jz .found

.next:
    inc rdx
    jmp .find_iter

.found:
    mov rsp, rax
    mov r15, rdx

    and rsp, -16

    test rsp, rsp
    jnz .t64_co1
    mov eax, 0x1ffff
    ud2
.t64_co1:
    mov rdx, [args.target]
    test rdx, rdx
    jnz .stay_tmp_paging
    mov eax, 0x3ffff
    ud2
.stay_tmp_paging:
    mov rcx, r15

    jmp rdx

align 8
gdt_start:
    dq 0x0000000000000000 ; Null
    dq 0x00cf9a000000ffff ; 32bit Code (0x08)
    dq 0x00cf92000000ffff ; 32bit Data (0x10)
    dq 0x00af9a000000ffff ; 64bit Code (0x18)
    dq 0x00af92000000ffff ; 64bit Data (0x20)
gdt_end:

gdtr_32:
    dw gdt_end - gdt_start - 1
    dd gdt_start

gdtr_64:
    dw gdt_end - gdt_start - 1
    dq gdt_start

times (0x9000 - 0x7500) - ($ - args) db 0

tmp_p5:
    dq tmp_p4 + 0b11
    times 511 dq 0

times (0xA000 - 0x7500) - ($ - args) db 0
tmp_p4:
    dq tmp_pdpt + 0b11
    times 511 dq 0

times (0xB000 - 0x7500) - ($ - args) db 0
tmp_pdpt:
    dq tmp_pd + 0b11
    times 511 dq 0

times (0xC000 - 0x7500) - ($ - args) db 0
tmp_pd:
    dq 0x0 + 0b10000111
    times 511 dq 0