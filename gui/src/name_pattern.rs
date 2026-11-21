use std::fmt::Write;
use std::path::Path;

use libks_ini::edit::Ini;
use ksmap::partition::Bounds;
use ksmap::seed::MapSeed;

use crate::get_level_author_and_name;

pub struct NamePattern {
    parts: Vec<PatternPart>
}

#[derive(Debug, PartialEq, Eq)]
enum PatternPart {
    Literal(String),
    LevelInfo(LevelInfoField),
    PartitionInfo(PartitionInfoField)
}

pub struct LevelInfo<'a> {
    pub dir_name: &'a str,
    pub author: &'a str,
    pub name: &'a str,
    pub seed: MapSeed
}

impl<'a> LevelInfo<'a> {
    pub fn new(ini: &'a Ini, level_dir: &'a Path, seed: MapSeed) -> Self {
        let (author, level_name) = get_level_author_and_name(ini);
        let dir_name = level_dir.file_name()
            .and_then(|name| name.to_str());

        Self {
            dir_name: dir_name.unwrap_or("Unknown"),
            author: author.unwrap_or("Unknown"),
            name: level_name.unwrap_or("Unknown"),
            seed
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum LevelInfoField {
    Directory,
    Author,
    Name,
    Seed,
}

pub struct PartitionInfo {
    pub index: usize,
    pub bounds: Bounds,
}

#[derive(Debug, PartialEq, Eq)]
enum PartitionInfoField {
    Index,
    Bounds,
    BoundsMin,
    BoundsMax,
    BoundsXMin,
    BoundsXMax,
    BoundsYMin,
    BoundsYMax,
}

fn get_pattern_part_by_name(name: &str) -> Option<PatternPart> {
    let part = match name {
        "dirname" => PatternPart::LevelInfo(LevelInfoField::Directory),
        "author"  => PatternPart::LevelInfo(LevelInfoField::Author),
        "name"    => PatternPart::LevelInfo(LevelInfoField::Name),
        "seed"    => PatternPart::LevelInfo(LevelInfoField::Seed),
        "index"   => PatternPart::PartitionInfo(PartitionInfoField::Index),
        "bounds"  => PatternPart::PartitionInfo(PartitionInfoField::Bounds),
        "min"     => PatternPart::PartitionInfo(PartitionInfoField::BoundsMin),
        "max"     => PatternPart::PartitionInfo(PartitionInfoField::BoundsMax),
        "xmin"    => PatternPart::PartitionInfo(PartitionInfoField::BoundsXMin),
        "xmax"    => PatternPart::PartitionInfo(PartitionInfoField::BoundsXMax),
        "ymin"    => PatternPart::PartitionInfo(PartitionInfoField::BoundsYMin),
        "ymax"    => PatternPart::PartitionInfo(PartitionInfoField::BoundsYMax),
        _         => return None
    };
    Some(part)
}

impl NamePattern {
    pub fn parse(spec: &str) -> Self {
        #[derive(PartialEq, Eq)]
        enum ParseState {
            Literal,
            AfterDollar,
            TokenName,
        }
        use ParseState::*;
        
        let mut parts = Vec::new();
        let mut state = Literal;
        let mut start = 0;
        
        let push_literal = |parts: &mut Vec<PatternPart>, start, end| {
            let literal = String::from(&spec[start..end]);
            if !literal.is_empty() {
                parts.push(PatternPart::Literal(literal.to_owned()))
            }
        };
        let maybe_push_token = |parts: &mut Vec<PatternPart>, start, end| {
            let token_name = &spec[start..end];
            if let Some(part) = get_pattern_part_by_name(token_name) {
                parts.push(part);
                true
            }
            else {
                false
            }
        };
        
        for (i, ch) in spec.char_indices() {
            state = match (state, ch) {
                (Literal, '$') => {
                    push_literal(&mut parts, start, i);
                    AfterDollar
                }
                (Literal, _) => Literal,
                (AfterDollar, '$') => {
                    push_literal(&mut parts, i, i + 1);
                    start = i + 1;
                    Literal
                }
                (AfterDollar, _) => {
                    start = i;
                    // Note: would need to check for single-character tokens here
                    TokenName
                }
                (TokenName, '$') => {
                    maybe_push_token(&mut parts, start, i);
                    start = i + 1;
                    AfterDollar
                }
                (TokenName, _) if ch.is_ascii_whitespace() => {
                    maybe_push_token(&mut parts, start, i);
                    start = i;
                    Literal
                }
                (TokenName, _) => {
                    if maybe_push_token(&mut parts, start, i + 1) {
                        start = i + 1;
                        Literal
                    }
                    else {
                        TokenName
                    }
                }
            };
        }
        
        if state == Literal {
            push_literal(&mut parts, start, spec.len());
        }
        
        Self { parts }
    }
    
    pub fn make_string(&self, level_info: &LevelInfo, partition_info: Option<PartitionInfo>) -> String {
        let mut out = String::new();
        
        for part in &self.parts {
            match part {
                PatternPart::Literal(s) => out.push_str(s),
                PatternPart::LevelInfo(LevelInfoField::Directory) => out.push_str(level_info.dir_name),
                PatternPart::LevelInfo(LevelInfoField::Author) => out.push_str(level_info.author),
                PatternPart::LevelInfo(LevelInfoField::Name) => out.push_str(level_info.name),
                PatternPart::LevelInfo(LevelInfoField::Seed) => { let _ = write!(out, "{}", level_info.seed); }
                PatternPart::PartitionInfo(field) if let Some(partition_info) = &partition_info => {
                    match field {
                        PartitionInfoField::Index      => { let _ = write!(out, "{}", partition_info.index); }
                        PartitionInfoField::Bounds     => { let _ = write!(out, "{}", partition_info.bounds); }
                        PartitionInfoField::BoundsMin  => { let _ = write!(out, "x{}y{}", partition_info.bounds.x.start, partition_info.bounds.y.start); }
                        PartitionInfoField::BoundsMax  => { let _ = write!(out, "x{}y{}", partition_info.bounds.x.end - 1, partition_info.bounds.y.end - 1); }
                        PartitionInfoField::BoundsXMin => { let _ = write!(out, "{}", partition_info.bounds.x.start); }
                        PartitionInfoField::BoundsXMax => { let _ = write!(out, "{}", partition_info.bounds.x.end - 1); }
                        PartitionInfoField::BoundsYMin => { let _ = write!(out, "{}", partition_info.bounds.y.start); }
                        PartitionInfoField::BoundsYMax => { let _ = write!(out, "{}", partition_info.bounds.y.end - 1); }
                    };
                }
                PatternPart::PartitionInfo(_) => {}
            }
        }
        
        out
    }
}

impl From<&str> for NamePattern {
    fn from(value: &str) -> Self {
        Self::parse(value)
    }
}

#[cfg(test)]
mod test {
    use ksmap::partition::Bounds;
    use ksmap::seed::MapSeed;

    use crate::name_pattern::{LevelInfo, LevelInfoField, NamePattern, PartitionInfo, PartitionInfoField, PatternPart};

    #[test]
    fn parser_handles_basic_spec() {
        let spec = "$author - $name";
        let pattern = NamePattern::parse(spec);
        assert_eq!(pattern.parts, [
            PatternPart::LevelInfo(LevelInfoField::Author),
            PatternPart::Literal(" - ".to_owned()),
            PatternPart::LevelInfo(LevelInfoField::Name),
        ]);
    }
    
    #[test]
    fn parser_handles_trailing_literal() {
        let spec = "$author - $nametrailing";
        let pattern = NamePattern::parse(spec);
        assert_eq!(pattern.parts, [
            PatternPart::LevelInfo(LevelInfoField::Author),
            PatternPart::Literal(" - ".to_owned()),
            PatternPart::LevelInfo(LevelInfoField::Name),
            PatternPart::Literal("trailing".to_owned()),
        ]);
    }
    
    #[test]
    fn parser_handles_repeated_dollar_sign() {
        let spec = "$$author - $$$name";
        let pattern = NamePattern::parse(spec);
        assert_eq!(pattern.parts, [
            PatternPart::Literal("$".to_owned()),
            PatternPart::Literal("author - ".to_owned()),
            PatternPart::Literal("$".to_owned()),
            PatternPart::LevelInfo(LevelInfoField::Name),
        ]);
    }
    
    #[test]
    fn parser_handles_trailing_dollar_sign() {
        let spec = "hello world$";
        let pattern = NamePattern::parse(spec);
        assert_eq!(pattern.parts, [
            PatternPart::Literal("hello world".to_owned()),
        ]);
    }
    
    #[test]
    fn parser_handles_unknown_token() {
        let spec = "$unknown trailing";
        let pattern = NamePattern::parse(spec);
        assert_eq!(pattern.parts, [
            PatternPart::Literal(" trailing".to_owned()),
        ]);
    }
    
    #[test]
    fn parser_recognizes_all_fields() {
        let spec = "$dirname$author$name$seed$index$bounds$min$max$xmin$xmax$ymin$ymax";
        let pattern = NamePattern::parse(spec);
        assert_eq!(pattern.parts, [
            PatternPart::LevelInfo(LevelInfoField::Directory),
            PatternPart::LevelInfo(LevelInfoField::Author),
            PatternPart::LevelInfo(LevelInfoField::Name),
            PatternPart::LevelInfo(LevelInfoField::Seed),
            PatternPart::PartitionInfo(PartitionInfoField::Index),
            PatternPart::PartitionInfo(PartitionInfoField::Bounds),
            PatternPart::PartitionInfo(PartitionInfoField::BoundsMin),
            PatternPart::PartitionInfo(PartitionInfoField::BoundsMax),
            PatternPart::PartitionInfo(PartitionInfoField::BoundsXMin),
            PatternPart::PartitionInfo(PartitionInfoField::BoundsXMax),
            PatternPart::PartitionInfo(PartitionInfoField::BoundsYMin),
            PatternPart::PartitionInfo(PartitionInfoField::BoundsYMax),
        ]);
    }
    
    #[test]
    fn all_fields_populate_correctly() {
        let level_info = LevelInfo {
            dir_name: "Directory name",
            author: "Level author",
            name: "Level name",
            seed: MapSeed { seed: 0x0123456789ABCDEF }
        };
        let partition_info = PartitionInfo {
            index: 0,
            bounds: Bounds {
                x: 100..201,
                y: 300..401,
            },
        };
        let spec = "$dirname\n$author\n$name\n$seed\n$index\n$bounds\n$min\n$max\n$xmin\n$xmax\n$ymin\n$ymax";
        let pattern = NamePattern::parse(spec);
        let result = pattern.make_string(&level_info, Some(partition_info));
        assert_eq!(result, "\
Directory name
Level author
Level name
0123456789ABCDEF
0
x100y300 to x200y400
x100y300
x200y400
100
200
300
400");
    }
    
    #[test]
    fn partition_fields_are_ignored_when_absent() {
        let level_info = LevelInfo {
            dir_name: "Directory name",
            author: "Level author",
            name: "Level name",
            seed: MapSeed { seed: 0x0123456789ABCDEF }
        };
        let spec = "$dirname\n$author\n$name\n$seed\n$index\n$bounds\n$min\n$max\n$xmin\n$xmax\n$ymin\n$ymax";
        let pattern = NamePattern::parse(spec);
        let result = pattern.make_string(&level_info, None);
        assert_eq!(result, "\
Directory name
Level author
Level name
0123456789ABCDEF







");
    }
}
