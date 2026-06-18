use libks_ini::edit::LogicalSection;

pub fn unpack_color(color: i32) -> [u8; 3] {
    let r = color & 0x000000FF;
    let g = (color & 0x0000FF00) >> 8;
    let b = (color & 0x00FF0000) >> 16;

    [r as u8, g as u8, b as u8]
}

pub trait LogicalSectionExt {
    fn get_i32_or(&self, key: &str, default: i32) -> i32;
    fn get_owned_or_default(&self, key: &str) -> String;
}

impl<'a> LogicalSectionExt for LogicalSection<'a> {
    /// Attempts to mimic how MMF parses and converts values from World.ini
    /// It's not perfectly accurate, but it's close.
    fn get_i32_or(&self, key: &str, default_val: i32) -> i32 {
        let Some(val) = self.get(key) else {
            return default_val
        };
        
        match get_ascii_number_prefix(val) {
            AsciiNumber::None => 0,
            AsciiNumber::Integer(s) => {
                str::parse::<i32>(s)
                    .unwrap_or(0)
            }
            AsciiNumber::Float(s) => {
                match str::parse::<f32>(s) {
                    Ok(f) if f.is_finite() => f as i32,
                    _ => 0,
                }
            }
        }
    }
    
    fn get_owned_or_default(&self, key: &str) -> String {
        self.get(key)
            .map(str::to_owned)
            .unwrap_or_default()
    }
}

enum AsciiNumber<'a> {
    None,
    Integer(&'a str),
    Float(&'a str),
}

fn get_ascii_number_prefix(s: &str) -> AsciiNumber<'_> {
    if s.is_empty() {
        return AsciiNumber::None;
    }
    
    let mut has_dot = false;
    let mut has_digit = false;
    let mut end = s.len();
    
    for (i, ch) in s.as_bytes().iter().enumerate() {
        match *ch as char {
            '0'..='9' => {
                has_digit = true;
            }
            '.' if !has_dot => {
                has_dot = true;
            }
            '+' if i == 0 => {}
            '-' if i == 0 => {}
            _ => {
                end = i;
                break;
            }
        }
    }

    if !has_digit {
        AsciiNumber::None
    }
    else if has_dot {
        AsciiNumber::Float(&s[..end])
    }
    else {
        AsciiNumber::Integer(&s[..end])
    }
}
