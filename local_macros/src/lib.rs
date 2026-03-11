use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, parse::{Parse, ParseStream}, Token, LitInt, Path, Result};

// 引数をパースするための構造体定義
struct IdtArgs {
    count: usize,
    handler: Path,
}

impl Parse for IdtArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let count_lit: LitInt = input.parse()?; // 255
        input.parse::<Token![,]>()?;            // ,
        let handler: Path = input.parse()?;     // InterruptHelper::func
        Ok(IdtArgs {
            count: count_lit.base10_parse()?,
            handler,
        })
    }
}


#[proc_macro]
pub fn generate_idt_entries(input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as IdtArgs);
    let count = args.count;
    let common_handler = &args.handler;

    let mut handlers = quote!();
    let mut table_elements = quote!();

    // CPUがエラーコードを積むインデックス
    let error_code_indices = [8, 10, 11, 12, 13, 14, 17, 21, 29, 30];

    for i in 0..=count {
        let name = format_ident!("handler_{}", i.to_string());
        let is_error = error_code_indices.contains(&(i as i32));

        // エラーコードがない例外は 0 を push して RawEntryArgs.error_code の位置を揃える
        let push_zero = if is_error {
            quote! { "", }
        } else {
            quote! { "push 0", }
        };

        handlers.extend(quote! {
            #[unsafe(naked)]
            pub unsafe extern "C" fn #name() {
                core::arch::naked_asm!(
                    "endbr64",
                    #push_zero
                    "push rax", "push rcx", "push rdx", "push rsi", "push rdi",
                    "push r8", "push r9", "push r10", "push r11",

                    "lea rdi, [rsp + 72]",
                    "mov rsi, {idx}",

                    "sub rsp, 40",
                    "call {target}",
                    "add rsp, 40",

                    "pop r11", "pop r10", "pop r9", "pop r8",
                    "pop rdi", "pop rsi", "pop rdx", "pop rcx", "pop rax",

                    "add rsp, 8",
                    "iretq",
                    idx = const #i,
                    target = sym #common_handler,
                );
            }
        });
        table_elements.extend(quote! { #name, });
    }

    quote! {
        pub mod macro_idt {
            use x86_64::structures::idt::{InterruptDescriptorTable, Entry, HandlerFunc};
            use x86_64::VirtAddr;

            #handlers

            pub static IDT_METHODS: [unsafe extern "C" fn(); #count + 1] = [ #table_elements ];

            pub fn init_all(idt: &mut InterruptDescriptorTable) {
                let ptr = idt as *mut InterruptDescriptorTable as *mut Entry<HandlerFunc>;
                for i in 0..=#count {
                    let addr = VirtAddr::new(IDT_METHODS[i] as u64);
                    unsafe {
                        let mut entry = Entry::<HandlerFunc>::missing();
                        entry.set_handler_addr(addr);
                        core::ptr::write(ptr.add(i), entry);
                    }
                }
            }
        }
    }.into()
}
