extern crate combine;
extern crate hostman_shared;
extern crate reqwest;
extern crate serde;
extern crate serde_json;
extern crate tokio;

use combine::{ParseError, Parser};
use hostman_shared::Table;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let body = reqwest::get("http://localhost:15332/get")
        .await?
        .text()
        .await?;

    let table: Table = serde_json::from_str(body.as_ref())?;

    println!("body = {:?}", table);

    Ok(())
}

#[derive(Debug, PartialEq)]
enum Line {
    Comment(String),
    Domain { ip: String, aliases: Vec<String> },
}

fn parse_domain<Input>() -> impl Parser<Input, Output = Line>
where
    Input: combine::stream::Stream<Token = char>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    // TODO(shalom) change this to a regex that matches for ipv4 and ipv6
    let ipv4 =
        combine::many1(combine::satisfy(|c: char| c.is_digit(10) || c == '.')).expected("ipv4");
    let ipv6 =
        combine::many1(combine::satisfy(|c: char| c.is_digit(16) || c == ':')).expected("ipv6");

    let alias = combine::many1(
        combine::many1(combine::satisfy(|c: char| !c.is_whitespace())).skip(
            combine::skip_many(combine::satisfy(|c: char| *&['\t', ' '].contains(&c)))
                .expected("alias delimiter"),
        ),
    );
    ipv4.or(ipv6)
        .skip(combine::parser::char::spaces())
        .and(alias)
        .map(|x| Line::Domain {
            ip: x.0,
            aliases: x.1,
        })
}

fn parse_comment<Input>() -> impl Parser<Input, Output = Line>
where
    Input: combine::stream::Stream<Token = char>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    combine::parser::char::char('#')
        .with(combine::many(combine::satisfy(|c| c != '\n')))
        .map(Line::Comment)
}

fn parse_line<Input>() -> impl Parser<Input, Output = Line>
where
    Input: combine::stream::Stream<Token = char>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    parse_comment().or(parse_domain())
}

#[cfg(test)]
mod tests {
    use super::*;
    use combine::{EasyParser};

    #[test]
    fn parse_domains_test() {
        let test: &'static str = "\
                                  127.0.0.1       localhost\n\
                                  ::1             localhost\n\
                                  127.0.1.1       domain.local pop-os\n\
                                  ";

        assert_eq!(
            combine::many1(parse_line().skip(combine::parser::char::char('\n')))
                .easy_parse(test)
                .map(|x| x.0),
            Ok(vec![
                Line::Domain {
                    ip: "127.0.0.1".to_owned(),
                    aliases: vec!["localhost".to_owned()]
                },
                Line::Domain {
                    ip: "::1".to_owned(),
                    aliases: vec!["localhost".to_owned()]
                },
                Line::Domain {
                    ip: "127.0.1.1".to_owned(),
                    aliases: vec!["domain.local".to_owned(), "pop-os".to_owned()]
                },
            ])
        )
    }

    #[test]
    fn parse_domains_with_comment_test() {
        let test: &'static str = "\
                                  127.0.0.1       localhost\n\
                                  # 127.0.0.1       localhost\n\
                                  ::1             localhost\n\
                                  127.0.1.1       domain.local pop-os\n\
                                  ";

        assert_eq!(
            combine::many1(parse_line().skip(combine::parser::char::char('\n')))
                .easy_parse(test)
                .map(|x| x.0),
            Ok(vec![
                Line::Domain {
                    ip: "127.0.0.1".to_owned(),
                    aliases: vec!["localhost".to_owned()]
                },
                Line::Comment(" 127.0.0.1       localhost".to_owned()),
                Line::Domain {
                    ip: "::1".to_owned(),
                    aliases: vec!["localhost".to_owned()]
                },
                Line::Domain {
                    ip: "127.0.1.1".to_owned(),
                    aliases: vec!["domain.local".to_owned(), "pop-os".to_owned()]
                },
            ])
        )
    }

    #[test]
    fn parse_domain_test() {
        assert_eq!(
            parse_domain()
                .easy_parse("127.0.0.1       localhost")
                .map(|x| x.0),
            Ok(Line::Domain {
                ip: "127.0.0.1".to_owned(),
                aliases: vec!["localhost".to_owned()]
            })
        );

        assert_eq!(
            parse_domain()
                .easy_parse("127.0.0.1       localhost local")
                .map(|x| x.0),
            Ok(Line::Domain {
                ip: "127.0.0.1".to_owned(),
                aliases: vec!["localhost".to_owned(), "local".to_owned()]
            })
        );

        assert_eq!(
            parse_domain()
                .easy_parse("::1       localhost local")
                .map(|x| x.0),
            Ok(Line::Domain {
                ip: "::1".to_owned(),
                aliases: vec!["localhost".to_owned(), "local".to_owned()]
            })
        );
    }
}
