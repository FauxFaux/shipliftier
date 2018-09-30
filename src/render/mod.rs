use cast::i64;
use failure::Error;
use failure::ResultExt;

use swagger::ArrayConstraints;
use swagger::DataType;
use swagger::Field;
use swagger::FullType;
use swagger::IntegerFormat;
use swagger::NamedType;

pub fn render(t: &NamedType) -> String {
    match t {
        NamedType::Name(name) => name.to_string(),
        NamedType::Simple(simple) => render_simple(simple),
        NamedType::Array { tee, .. } => format!("Vec<{}>", render(tee)),
        NamedType::Unknown => "()".to_string(),
    }
}

fn render_simple(simple: &DataType) -> String {
    use self::DataType::*;
    use self::IntegerFormat::*;

    match simple {
        String { .. } => "String".to_string(),
        Integer {
            min, max, format, ..
        } => {
            let unsigned = min.unwrap_or(-1) >= 0 || match format {
                U8 | U16 | U32 | U64 => true,
                _ => false,
            };
            let format_bits = match format {
                U8 | I8 => 8,
                U16 | I16 => 16,
                U32 | I32 => 32,
                U64 | I64 => 64,
                Unspecified => 64,
            };

            let max_bits = if let Some(max) = *max {
                if max > i64(::std::i32::MAX) {
                    64
                } else if max > i64(::std::i16::MAX) {
                    32
                } else if max > i64(::std::i8::MAX) {
                    16
                } else {
                    8
                }
            } else {
                0
            };

            format!(
                "{}{}",
                if unsigned { "u" } else { "i" },
                format_bits.max(max_bits)
            )
        }

        other => format!("(/* unsupported type: {:?} */)", other),
    }
}
