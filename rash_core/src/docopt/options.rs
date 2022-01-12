use regex::Regex;

lazy_static! {
    static ref RE_DEFAULT_VALUE: Regex = Regex::new(r"\[default: (.*)\]").unwrap();
}

#[derive(Debug, PartialEq)]
pub enum OptionArg {
    Simple {
        short: Option<String>,
        long: Option<String>,
    },
    WithParam {
        short: Option<String>,
        long: Option<String>,
        default_value: Option<String>,
    },
}

#[derive(Debug, PartialEq)]
pub struct Options(Vec<OptionArg>);

impl Options {
    pub fn parse(doc: &str) -> Self {
        Options(
            doc.split('\n')
                .into_iter()
                .filter_map(|line| {
                    let trimed = line.trim_start();
                    if trimed.starts_with('-') {
                        Some(trimed)
                    } else {
                        None
                    }
                })
                .map(|o| {
                    let (option, description) =
                        if let Some((option, description)) = o.split_once("  ") {
                            (option, description)
                        } else {
                            (o, "")
                        };

                    let mut short: Option<String> = None;
                    let mut long: Option<String> = None;
                    let mut is_with_param = false;

                    for w in option.replace(',', " ").replace('=', " ").split(' ') {
                        if w.starts_with("--") {
                            long = Some(w.to_string());
                        } else if w.starts_with('-') {
                            short = Some(w.to_string());
                        } else {
                            is_with_param = true;
                        }
                    }
                    if is_with_param {
                        let default_value =
                            if let Some(cap) = RE_DEFAULT_VALUE.captures(description) {
                                cap.get(1).map(|x| x.as_str().to_string())
                            } else {
                                None
                            };
                        OptionArg::WithParam {
                            short,
                            long,
                            default_value,
                        }
                    } else {
                        OptionArg::Simple { short, long }
                    }
                })
                .collect::<Vec<OptionArg>>(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_options_parse() {
        let file = r#"
Usage: my_program.py [-hso FILE] [--quiet | --verbose] [INPUT ...]

-h --help    show this
-s --sorted  sorted output
-o FILE      specify output file [default: ./test.txt]
--quiet      print less text
--verbose    print more text
"#;

        let result = Options::parse(file);

        assert_eq!(
            result,
            Options(vec![
                OptionArg::Simple {
                    short: Some("-h".to_string()),
                    long: Some("--help".to_string()),
                },
                OptionArg::Simple {
                    short: Some("-s".to_string()),
                    long: Some("--sorted".to_string()),
                },
                OptionArg::WithParam {
                    short: Some("-o".to_string()),
                    long: None,
                    default_value: Some("./test.txt".to_string()),
                },
                OptionArg::Simple {
                    short: None,
                    long: Some("--quiet".to_string()),
                },
                OptionArg::Simple {
                    short: None,
                    long: Some("--verbose".to_string()),
                },
            ])
        )
    }
}
