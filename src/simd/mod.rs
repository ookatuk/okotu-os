use core::simd::*;
use paste::paste;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdLevel { None, Sse4, Avx2, Avx512 }

// --- 基礎マクロ: 算術 ---
macro_rules! def_arith {
    ($op:ident, $inner:ty, $s:ty, $name:ident, $suffix:literal, $target:literal, $lanes:expr) => {
        paste! {
            #[target_feature(enable = $target)]
            #[allow(non_snake_case)]
            pub unsafe fn [<$name _ $op _ $suffix>](a: &mut [$inner], b: &[$inner], len: usize) {
                let mut i = 0;
                while i < len {
                    let remain = len - i;
                    let mask = Mask::from_bitmask(if remain >= $lanes { !0u64 } else { (1u64 << remain) - 1 });
                    let va = <$s>::load_select(&a[i..], mask, <$s>::splat(0 as $inner));
                    let vb = <$s>::load_select(&b[i..], mask, <$s>::splat(0 as $inner));
                    let res = def_arith!(@simd $op, va, vb);
                    res.store_select(&mut a[i..], mask);
                    i += $lanes;
                }
            }
        }
    };
    (@simd add, $va:expr, $vb:expr) => { $va + $vb };
    (@simd sub, $va:expr, $vb:expr) => { $va - $vb };
    (@simd mul, $va:expr, $vb:expr) => { $va * $vb };
    (@simd div, $va:expr, $vb:expr) => { $va / $vb };
    (@simd rem, $va:expr, $vb:expr) => { $va % $vb };
}

// --- 基礎マクロ: 論理 ---
macro_rules! def_logic {
    ($op:ident, $inner:ty, $s:ty, $name:ident, $suffix:literal, $target:literal, $lanes:expr, $sym:tt) => {
        paste! {
            #[target_feature(enable = $target)]
            pub unsafe fn [<$name _ $op _ $suffix>](a: &mut [$inner], b: &[$inner], len: usize) {
                let mut i = 0;
                while i < len {
                    let remain = len - i;
                    let mask = Mask::from_bitmask(if remain >= $lanes { !0u64 } else { (1u64 << remain) - 1 });
                    let va = <$s>::load_select(&a[i..], mask, <$s>::splat(0 as $inner));
                    let vb = <$s>::load_select(&b[i..], mask, <$s>::splat(0 as $inner));
                    (va $sym vb).store_select(&mut a[i..], mask);
                    i += $lanes;
                }
            }
        }
    };
}

// --- 基礎マクロ: 単項 ---
macro_rules! def_not {
    ($inner:ty, $s:ty, $name:ident, $suffix:literal, $target:literal, $lanes:expr) => {
        paste! {
            #[target_feature(enable = $target)]
            pub unsafe fn [<$name _not_ $suffix>](a: &mut [$inner], len: usize) {
                let mut i = 0;
                while i < len {
                    let remain = len - i;
                    let mask = Mask::from_bitmask(if remain >= $lanes { !0u64 } else { (1u64 << remain) - 1 });
                    let v = <$s>::load_select(&a[i..], mask, <$s>::splat(0 as $inner));
                    (!v).store_select(&mut a[i..], mask);
                    i += $lanes;
                }
            }
        }
    };
}

// --- 基礎マクロ: シフト ---
macro_rules! def_shift {
    ($op:ident, $inner:ty, $s:ty, $name:ident, $suffix:literal, $target:literal, $lanes:expr) => {
        paste! {
            #[target_feature(enable = $target)]
            pub unsafe fn [<$name _ $op _ $suffix>](a: &mut [$inner], n: u32, len: usize) {
                let mut i = 0;
                let vn = <$s>::splat(n as $inner);
                #[allow(unused)]
                let v_bits = <$s>::splat((core::mem::size_of::<$inner>() * 8) as $inner);
                while i < len {
                    let remain = len - i;
                    let mask = Mask::from_bitmask(if remain >= $lanes { !0u64 } else { (1u64 << remain) - 1 });
                    let v = <$s>::load_select(&a[i..], mask, <$s>::splat(0 as $inner));
                    let res = def_shift!(@simd $op, v, vn, v_bits);
                    res.store_select(&mut a[i..], mask);
                    i += $lanes;
                }
            }
        }
    };
    (@simd shl, $v:expr, $vn:expr, $bits:expr) => { $v << $vn };
    (@simd shr, $v:expr, $vn:expr, $bits:expr) => { $v >> $vn };
    (@simd rotl, $v:expr, $vn:expr, $bits:expr) => { ($v << $vn) | ($v >> ($bits - $vn)) };
    (@simd rotr, $v:expr, $vn:expr, $bits:expr) => { ($v >> $vn) | ($v << ($bits - $vn)) };
}

// --- 一括展開 ---
macro_rules! setup_simd_types {
    { $( $name:ident => ($inner:ty, $kind:ident, $s128:ty, $s256:ty, $s512:ty) ),* $(,)? } => {
        $(
            paste! {
                // 算術 5種
                def_arith!(add, $inner, $s128, $name, 128, "sse4.1", (128/8/core::mem::size_of::<$inner>()));
                def_arith!(add, $inner, $s256, $name, 256, "avx2",   (256/8/core::mem::size_of::<$inner>()));
                def_arith!(add, $inner, $s512, $name, 512, "avx512f", (512/8/core::mem::size_of::<$inner>()));
                pub fn [<$name:lower _batch_add>](a: &mut [$inner], b: &[$inner], level: SimdLevel) {
                    let len = a.len().min(b.len());
                    match level {
                        SimdLevel::Avx512 => unsafe { [<$name _add_512>](a, b, len) },
                        SimdLevel::Avx2   => unsafe { [<$name _add_256>](a, b, len) },
                        SimdLevel::Sse4   => unsafe { [<$name _add_128>](a, b, len) },
                        _ => for i in 0..len { setup_simd_types!(@scalar_ $kind, add, a, b, i); }
                    }
                }

                def_arith!(sub, $inner, $s128, $name, 128, "sse4.1", (128/8/core::mem::size_of::<$inner>()));
                def_arith!(sub, $inner, $s256, $name, 256, "avx2",   (256/8/core::mem::size_of::<$inner>()));
                def_arith!(sub, $inner, $s512, $name, 512, "avx512f", (512/8/core::mem::size_of::<$inner>()));
                pub fn [<$name:lower _batch_sub>](a: &mut [$inner], b: &[$inner], level: SimdLevel) {
                    let len = a.len().min(b.len());
                    match level {
                        SimdLevel::Avx512 => unsafe { [<$name _sub_512>](a, b, len) },
                        SimdLevel::Avx2   => unsafe { [<$name _sub_256>](a, b, len) },
                        SimdLevel::Sse4   => unsafe { [<$name _sub_128>](a, b, len) },
                        _ => for i in 0..len { setup_simd_types!(@scalar_ $kind, sub, a, b, i); }
                    }
                }

                def_arith!(mul, $inner, $s128, $name, 128, "sse4.1", (128/8/core::mem::size_of::<$inner>()));
                def_arith!(mul, $inner, $s256, $name, 256, "avx2",   (256/8/core::mem::size_of::<$inner>()));
                def_arith!(mul, $inner, $s512, $name, 512, "avx512f", (512/8/core::mem::size_of::<$inner>()));
                pub fn [<$name:lower _batch_mul>](a: &mut [$inner], b: &[$inner], level: SimdLevel) {
                    let len = a.len().min(b.len());
                    match level {
                        SimdLevel::Avx512 => unsafe { [<$name _mul_512>](a, b, len) },
                        SimdLevel::Avx2   => unsafe { [<$name _mul_256>](a, b, len) },
                        SimdLevel::Sse4   => unsafe { [<$name _mul_128>](a, b, len) },
                        _ => for i in 0..len { setup_simd_types!(@scalar_ $kind, mul, a, b, i); }
                    }
                }

                def_arith!(div, $inner, $s128, $name, 128, "sse4.1", (128/8/core::mem::size_of::<$inner>()));
                def_arith!(div, $inner, $s256, $name, 256, "avx2",   (256/8/core::mem::size_of::<$inner>()));
                def_arith!(div, $inner, $s512, $name, 512, "avx512f", (512/8/core::mem::size_of::<$inner>()));
                pub fn [<$name:lower _batch_div>](a: &mut [$inner], b: &[$inner], level: SimdLevel) {
                    let len = a.len().min(b.len());
                    match level {
                        SimdLevel::Avx512 => unsafe { [<$name _div_512>](a, b, len) },
                        SimdLevel::Avx2   => unsafe { [<$name _div_256>](a, b, len) },
                        SimdLevel::Sse4   => unsafe { [<$name _div_128>](a, b, len) },
                        _ => for i in 0..len { setup_simd_types!(@scalar_ $kind, div, a, b, i); }
                    }
                }

                def_arith!(rem, $inner, $s128, $name, 128, "sse4.1", (128/8/core::mem::size_of::<$inner>()));
                def_arith!(rem, $inner, $s256, $name, 256, "avx2",   (256/8/core::mem::size_of::<$inner>()));
                def_arith!(rem, $inner, $s512, $name, 512, "avx512f", (512/8/core::mem::size_of::<$inner>()));
                pub fn [<$name:lower _batch_rem>](a: &mut [$inner], b: &[$inner], level: SimdLevel) {
                    let len = a.len().min(b.len());
                    match level {
                        SimdLevel::Avx512 => unsafe { [<$name _rem_512>](a, b, len) },
                        SimdLevel::Avx2   => unsafe { [<$name _rem_256>](a, b, len) },
                        SimdLevel::Sse4   => unsafe { [<$name _rem_128>](a, b, len) },
                        _ => for i in 0..len { setup_simd_types!(@scalar_ $kind, rem, a, b, i); }
                    }
                }

                setup_simd_types!(@int_only $kind, $name, $inner, $s128, $s256, $s512);
            }
        )*
    };

    // 整数のみの演算
    (@int_only int, $name:ident, $inner:ty, $s128:ty, $s256:ty, $s512:ty) => {
        paste! {
            // Logic 3種 + Not
            def_logic!(bitand, $inner, $s128, $name, 128, "sse4.1", (128/8/core::mem::size_of::<$inner>()), &);
            def_logic!(bitand, $inner, $s256, $name, 256, "avx2",   (256/8/core::mem::size_of::<$inner>()), &);
            def_logic!(bitand, $inner, $s512, $name, 512, "avx512f", (512/8/core::mem::size_of::<$inner>()), &);
            pub fn [<$name:lower _batch_bitand>](a: &mut [$inner], b: &[$inner], level: SimdLevel) {
                let len = a.len().min(b.len());
                match level {
                    SimdLevel::Avx512 => unsafe { [<$name _bitand_512>](a, b, len) },
                    SimdLevel::Avx2   => unsafe { [<$name _bitand_256>](a, b, len) },
                    SimdLevel::Sse4   => unsafe { [<$name _bitand_128>](a, b, len) },
                    _ => for i in 0..len { a[i] &= b[i]; }
                }
            }

            def_logic!(bitor, $inner, $s128, $name, 128, "sse4.1", (128/8/core::mem::size_of::<$inner>()), |);
            def_logic!(bitor, $inner, $s256, $name, 256, "avx2",   (256/8/core::mem::size_of::<$inner>()), |);
            def_logic!(bitor, $inner, $s512, $name, 512, "avx512f", (512/8/core::mem::size_of::<$inner>()), |);
            pub fn [<$name:lower _batch_bitor>](a: &mut [$inner], b: &[$inner], level: SimdLevel) {
                let len = a.len().min(b.len());
                match level {
                    SimdLevel::Avx512 => unsafe { [<$name _bitor_512>](a, b, len) },
                    SimdLevel::Avx2   => unsafe { [<$name _bitor_256>](a, b, len) },
                    SimdLevel::Sse4   => unsafe { [<$name _bitor_128>](a, b, len) },
                    _ => for i in 0..len { a[i] |= b[i]; }
                }
            }

            def_logic!(bitxor, $inner, $s128, $name, 128, "sse4.1", (128/8/core::mem::size_of::<$inner>()), ^);
            def_logic!(bitxor, $inner, $s256, $name, 256, "avx2",   (256/8/core::mem::size_of::<$inner>()), ^);
            def_logic!(bitxor, $inner, $s512, $name, 512, "avx512f", (512/8/core::mem::size_of::<$inner>()), ^);
            pub fn [<$name:lower _batch_bitxor>](a: &mut [$inner], b: &[$inner], level: SimdLevel) {
                let len = a.len().min(b.len());
                match level {
                    SimdLevel::Avx512 => unsafe { [<$name _bitxor_512>](a, b, len) },
                    SimdLevel::Avx2   => unsafe { [<$name _bitxor_256>](a, b, len) },
                    SimdLevel::Sse4   => unsafe { [<$name _bitxor_128>](a, b, len) },
                    _ => for i in 0..len { a[i] ^= b[i]; }
                }
            }

            def_not!($inner, $s128, $name, 128, "sse4.1", (128/8/core::mem::size_of::<$inner>()));
            def_not!($inner, $s256, $name, 256, "avx2",   (256/8/core::mem::size_of::<$inner>()));
            def_not!($inner, $s512, $name, 512, "avx512f", (512/8/core::mem::size_of::<$inner>()));
            pub fn [<$name:lower _batch_not>](a: &mut [$inner], level: SimdLevel) {
                let len = a.len();
                match level {
                    SimdLevel::Avx512 => unsafe { [<$name _not_512>](a, len) },
                    SimdLevel::Avx2   => unsafe { [<$name _not_256>](a, len) },
                    SimdLevel::Sse4   => unsafe { [<$name _not_128>](a, len) },
                    _ => for i in 0..len { a[i] = !a[i]; }
                }
            }

            // Shift/Rotate 4種
            def_shift!(shl, $inner, $s128, $name, 128, "sse4.1", (128/8/core::mem::size_of::<$inner>()));
            def_shift!(shl, $inner, $s256, $name, 256, "avx2",   (256/8/core::mem::size_of::<$inner>()));
            def_shift!(shl, $inner, $s512, $name, 512, "avx512f", (512/8/core::mem::size_of::<$inner>()));
            pub fn [<$name:lower _batch_shl>](a: &mut [$inner], n: u32, level: SimdLevel) {
                let len = a.len();
                match level {
                    SimdLevel::Avx512 => unsafe { [<$name _shl_512>](a, n, len) },
                    SimdLevel::Avx2   => unsafe { [<$name _shl_256>](a, n, len) },
                    SimdLevel::Sse4   => unsafe { [<$name _shl_128>](a, n, len) },
                    _ => for i in 0..len { a[i] <<= n; }
                }
            }

            def_shift!(shr, $inner, $s128, $name, 128, "sse4.1", (128/8/core::mem::size_of::<$inner>()));
            def_shift!(shr, $inner, $s256, $name, 256, "avx2",   (256/8/core::mem::size_of::<$inner>()));
            def_shift!(shr, $inner, $s512, $name, 512, "avx512f", (512/8/core::mem::size_of::<$inner>()));
            pub fn [<$name:lower _batch_shr>](a: &mut [$inner], n: u32, level: SimdLevel) {
                let len = a.len();
                match level {
                    SimdLevel::Avx512 => unsafe { [<$name _shr_512>](a, n, len) },
                    SimdLevel::Avx2   => unsafe { [<$name _shr_256>](a, n, len) },
                    SimdLevel::Sse4   => unsafe { [<$name _shr_128>](a, n, len) },
                    _ => for i in 0..len { a[i] >>= n; }
                }
            }

            def_shift!(rotl, $inner, $s128, $name, 128, "sse4.1", (128/8/core::mem::size_of::<$inner>()));
            def_shift!(rotl, $inner, $s256, $name, 256, "avx2",   (256/8/core::mem::size_of::<$inner>()));
            def_shift!(rotl, $inner, $s512, $name, 512, "avx512f", (512/8/core::mem::size_of::<$inner>()));
            pub fn [<$name:lower _batch_rotl>](a: &mut [$inner], n: u32, level: SimdLevel) {
                let len = a.len();
                match level {
                    SimdLevel::Avx512 => unsafe { [<$name _rotl_512>](a, n, len) },
                    SimdLevel::Avx2   => unsafe { [<$name _rotl_256>](a, n, len) },
                    SimdLevel::Sse4   => unsafe { [<$name _rotl_128>](a, n, len) },
                    _ => for i in 0..len { a[i] = a[i].rotate_left(n); }
                }
            }

            def_shift!(rotr, $inner, $s128, $name, 128, "sse4.1", (128/8/core::mem::size_of::<$inner>()));
            def_shift!(rotr, $inner, $s256, $name, 256, "avx2",   (256/8/core::mem::size_of::<$inner>()));
            def_shift!(rotr, $inner, $s512, $name, 512, "avx512f", (512/8/core::mem::size_of::<$inner>()));
            pub fn [<$name:lower _batch_rotr>](a: &mut [$inner], n: u32, level: SimdLevel) {
                let len = a.len();
                match level {
                    SimdLevel::Avx512 => unsafe { [<$name _rotr_512>](a, n, len) },
                    SimdLevel::Avx2   => unsafe { [<$name _rotr_256>](a, n, len) },
                    SimdLevel::Sse4   => unsafe { [<$name _rotr_128>](a, n, len) },
                    _ => for i in 0..len { a[i] = a[i].rotate_right(n); }
                }
            }
        }
    };
    (@int_only float, $($any:tt)*) => {};

    // スカラーフォールバックの詳細定義
    (@scalar_ int, add, $a:ident, $b:ident, $i:ident) => { $a[$i] = $a[$i].wrapping_add($b[$i]) };
    (@scalar_ int, sub, $a:ident, $b:ident, $i:ident) => { $a[$i] = $a[$i].wrapping_sub($b[$i]) };
    (@scalar_ int, mul, $a:ident, $b:ident, $i:ident) => { $a[$i] = $a[$i].wrapping_mul($b[$i]) };
    (@scalar_ int, div, $a:ident, $b:ident, $i:ident) => { if $b[$i] != 0 { $a[$i] /= $b[$i] } };
    (@scalar_ int, rem, $a:ident, $b:ident, $i:ident) => { if $b[$i] != 0 { $a[$i] %= $b[$i] } };
    (@scalar_ float, add, $a:ident, $b:ident, $i:ident) => { $a[$i] += $b[$i] };
    (@scalar_ float, sub, $a:ident, $b:ident, $i:ident) => { $a[$i] -= $b[$i] };
    (@scalar_ float, mul, $a:ident, $b:ident, $i:ident) => { $a[$i] *= $b[$i] };
    (@scalar_ float, div, $a:ident, $b:ident, $i:ident) => { $a[$i] /= $b[$i] };
    (@scalar_ float, rem, $a:ident, $b:ident, $i:ident) => { $a[$i] %= $b[$i] };
}

// --- 最終展開 ---
setup_simd_types! {
    SimdU64 => (u64, int, u64x2, u64x4, u64x8),
    SimdU32 => (u32, int, u32x4, u32x8, u32x16),
    SimdU16 => (u16, int, u16x8, u16x16, u16x32),
    SimdU8  => (u8,  int, u8x16, u8x32, u8x64),
    SimdI64 => (i64, int, i64x2, i64x4, i64x8),
    SimdI32 => (i32, int, i32x4, i32x8, i32x16),
    SimdI16 => (i16, int, i16x8, i16x16, i16x32),
    SimdI8  => (i8,  int, i8x16, i8x32, i8x64),
    SimdF64 => (f64, float, f64x2, f64x4, f64x8),
    SimdF32 => (f32, float, f32x4, f32x8, f32x16),
}