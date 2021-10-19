use crate::{fmt::IsDisplay, panic_val::PanicVal, utils::WasTruncated};

#[cold]
#[inline(never)]
#[track_caller]
pub const fn concat_panic(args: &[&[PanicVal<'_>]]) -> ! {
    // The panic message capacity starts small and gets larger each time,
    // so that platforms with smaller stacks can call this at runtime.
    //
    // Also, given that most(?) panic messages are smaller than 1024 bytes long,
    // it's not going to be any less efficient in the common case.
    if let Err(_) = panic_inner::<1024>(args) {}

    if let Err(_) = panic_inner::<{ 1024 * 6 }>(args) {}

    match panic_inner::<MAX_PANIC_MSG_LEN>(args) {
        Ok(x) => match x {},
        Err(_) => panic!(
            "\
            unreachable:\n\
            the `write_panicval_to_buffer` macro must not return Err when \
            $capacity == $max_capacity\
        "
        ),
    }
}

// this should probably be smaller on platforms where this
// const fn is called at runtime, and the stack is finy.
const MAX_PANIC_MSG_LEN: usize = 32768;

macro_rules! write_panicval_to_buffer {
    (
        $outer_label:lifetime,
        $buffer:ident,
        $len:ident,
        ($capacity:expr, $max_capacity:expr),
        $panicval:expr
        $(,)*
    ) => {
        let rem_space = $capacity - $len;
        let arg = $panicval;
        let (mut lpad, mut rpad, string, is_display, was_truncated) = arg.__string(rem_space);
        let trunc_len = was_truncated.get_length(string);

        while lpad != 0 {
            __write_array! {$buffer, $len, b' '}
            lpad -= 1;
        }

        if let IsDisplay::Yes = is_display {
            let mut i = 0;
            while i < trunc_len {
                __write_array! {$buffer, $len, string[i]}
                i += 1;
            }
        } else if rem_space != 0 {
            __write_array! {$buffer, $len, b'"'}
            let mut i = 0;
            while i < trunc_len {
                use crate::debug_str_fmt::{hex_as_ascii, ForEscaping};

                let c = string[i];
                let mut written_c = c;
                if ForEscaping::is_escaped(c) {
                    __write_array! {$buffer, $len, b'\\'}
                    if ForEscaping::is_backslash_escaped(c) {
                        written_c = ForEscaping::get_backslash_escape(c);
                    } else {
                        __write_array! {$buffer, $len, b'x'}
                        __write_array! {$buffer, $len, hex_as_ascii(c >> 4)}
                        written_c = hex_as_ascii(c & 0b1111);
                    };
                }
                __write_array! {$buffer, $len, written_c}

                i += 1;
            }
            if let WasTruncated::No = was_truncated {
                __write_array_checked! {$buffer, $len, b'"'}
            }
        }

        while rpad != 0 {
            __write_array! {$buffer, $len, b' '}
            rpad -= 1;
        }

        if let WasTruncated::Yes(_) = was_truncated {
            if $capacity < $max_capacity {
                return Err(NotEnoughSpace);
            } else {
                break $outer_label;
            }
        }
    };
}

macro_rules! write_to_buffer {
    ($args:ident, $buffer:ident, $len:ident, $wptb_args:tt $(,)*) => {
        let mut args = $args;
        'outer: while let [mut outer, ref nargs @ ..] = args {
            while let [arg, nouter @ ..] = outer {
                match arg.var {
                    #[cfg(feature = "all_items")]
                    crate::panic_val::PanicVariant::Slice(slice) => {
                        let mut iter = slice.iter();

                        'iter: loop {
                            let (two_args, niter) = iter.next();

                            let mut two_args: &[_] = &two_args;
                            while let [arg, ntwo_args @ ..] = two_args {
                                write_panicval_to_buffer! {'outer, $buffer, $len, $wptb_args, arg}
                                two_args = ntwo_args;
                            }

                            match niter {
                                Some(x) => iter = x,
                                None => break 'iter,
                            }
                        }
                    }
                    _ => {
                        write_panicval_to_buffer! {'outer, $buffer, $len, $wptb_args, arg}
                    }
                }

                outer = nouter;
            }
            args = nargs;
        }
    };
}

#[cold]
#[inline(never)]
#[track_caller]
const fn panic_inner<const LEN: usize>(args: &[&[PanicVal<'_>]]) -> Result<Never, NotEnoughSpace> {
    let mut buffer = [0u8; LEN];
    let mut len = 0usize;

    write_to_buffer! {
        args,
        buffer,
        len,
        (LEN, MAX_PANIC_MSG_LEN),
    }

    unsafe {
        let str = core::str::from_utf8_unchecked(&buffer);
        panic!("{}", str)
    }
}

#[doc(hidden)]
#[derive(Debug)]
pub struct NotEnoughSpace;
enum Never {}

#[cfg(feature = "test")]
use crate::test_utils::ArrayString;

#[doc(hidden)]
#[cfg(feature = "test")]
pub fn format_panic_message(
    args: &[&[PanicVal<'_>]],
    capacity: usize,
    max_capacity: usize,
) -> Result<ArrayString<1024>, NotEnoughSpace> {
    let mut buffer = [0u8; 1024];
    let mut len = 0usize;
    {
        // intentionally shadowed
        let buffer = &mut buffer[..capacity];

        write_to_buffer! {
            args,
            buffer,
            len,
            (capacity, max_capacity),
        }
    }

    Ok(ArrayString { buffer, len })
}
