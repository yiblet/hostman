extern crate combine;
extern crate hostman_shared;
extern crate pnet;
extern crate reqwest;
extern crate serde;
extern crate serde_json;
extern crate sys_info;
extern crate tokio;

use combine::{ParseError, Parser};
use hostman_shared::{Current, Table};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file_loc = "hosts";
    let table = read_hosts(file_loc).await;

    let body;
    if let Some(current) = table.map(|table| table.current)? {
        let req: String = format!(
            "http://localhost:15332/update/{}/{}/",
            &current.host, &current.ips[0],
        );

        body = reqwest::get(&req).await?.text().await?;
    } else {
        body = reqwest::get("http://localhost:15332/get")
            .await?
            .text()
            .await?;
    }

    let table: Table = serde_json::from_str(body.as_ref())?;
    println!("body = {:?}", table);
    write_table(&table, file_loc).await?;
    Ok(())
}

async fn write_table(table: &Table, file_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    use tokio::prelude::*;
    let mut contents = vec![];
    let mut file = tokio::fs::File::open(file_name).await?;

    file.read_to_end(&mut contents).await?;
    let file_contents = std::str::from_utf8(contents.as_slice())?;
    let lines = gen_lines(file_contents)?;
    let range_opt = splice_hostman_lines(&lines);

    let mut out_file = tokio::fs::File::create(file_name).await?;

    match range_opt {
        // TODO(shalom) clean this up
        Some(range) => {
            // TODO(shalom) possible bug using `lines()` means that we remove the last '\r\n'
            // in each line so for lines that include a '\r' that character is lost
            for line in file_contents.lines().take(range.start) {
                out_file.write(line.as_bytes()).await?;
                out_file.write("\n".as_bytes()).await?;
            }
            for (host, ip) in table.host_mapping.iter() {
                let mut buffer = String::new();
                buffer += ip;
                buffer.push('\t');
                buffer += host;
                buffer.push('\n');
                out_file.write(buffer.as_bytes()).await?;
            }
            for line in file_contents.lines().skip(range.end) {
                out_file.write(line.as_bytes()).await?;
                out_file.write("\n".as_bytes()).await?;
            }
        }
        _ => {
            for line in file_contents.lines() {
                out_file.write(line.as_bytes()).await?;
                out_file.write("\n".as_bytes()).await?;
            }
            out_file.write("# hostman:start\n".as_bytes()).await?;
            for (host, ip) in table.host_mapping.iter() {
                let mut buffer = String::new();
                buffer += ip;
                buffer.push('\t');
                buffer += host;
                buffer.push('\n');
                out_file.write(buffer.as_bytes()).await?;
            }
            out_file.write("# hostman:end\n".as_bytes()).await?;
        }
    }

    out_file.flush().await?;
    Ok(())
}

#[derive(Debug, PartialEq, Clone)]
enum Line {
    Comment(String),
    Domain { ip: String, aliases: Vec<String> },
}

impl Line {
    fn is_comment(&self) -> bool {
        match self {
            Self::Comment(_) => true,
            _ => false,
        }
    }
    fn is_domain(&self) -> bool {
        match self {
            Self::Domain { .. } => true,
            _ => false,
        }
    }
}

type Lines = Vec<Line>;

#[inline]
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

#[inline]
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

fn parse_lines<Input>() -> impl Parser<Input, Output = Lines>
where
    Input: combine::stream::Stream<Token = char>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    combine::many(parse_line().skip(combine::parser::char::char('\n')))
}

fn parse_hostman_lines<Input>() -> impl Parser<Input, Output = Lines>
where
    Input: combine::stream::Stream<Token = char>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    let parse_line_br = || parse_line().skip(combine::parser::char::char('\n'));

    // combine::many(parse_line_br())

    combine::many::<Vec<_>, _, _>(combine::attempt(parse_line_br().then(|line: Line| {
        println!("{:?}", line);
        if line != Line::Comment(" hostman:start".to_owned()) {
            combine::value(line).left()
        } else {
            combine::unexpected_any("failure").right()
        }
    })))
    .skip(parse_line_br())
    .with(combine::many::<Vec<_>, _, _>(combine::attempt(
        parse_line_br().then(|line: Line| {
            println!("{:?}", line);
            if line != Line::Comment(" hostman:end".to_owned()) {
                combine::value(line).left()
            } else {
                combine::unexpected_any("failure").right()
            }
        }),
    )))
}

fn splice_hostman_lines(lines: &Lines) -> Option<std::ops::Range<usize>> {
    let start_idx = lines
        .iter()
        .enumerate()
        .find(|(_, line)| *line == &Line::Comment(" hostman:start".to_owned()))?
        .0;

    let end_idx = lines
        .iter()
        .enumerate()
        .find(|(_, line)| *line == &Line::Comment(" hostman:end".to_owned()))?
        .0;

    Some(start_idx + 1..end_idx)
}

fn gen_lines(file_contents: &str) -> Result<Lines, Box<dyn std::error::Error>> {
    use combine::stream::buffered;
    use combine::stream::position;
    let positioned =
        position::Stream::with_positioner(file_contents, position::SourcePosition::default());
    let mut stream = buffered::Stream::new(positioned, 256);

    parse_lines()
        .parse_stream(&mut stream)
        .into_result()
        .map_err(|e| format!("{:?}", e).into())
        .map(|tup| tup.0)
}

fn gen_table(file_contents: &str) -> Result<Table, Box<dyn std::error::Error>> {
    let lines = gen_lines(file_contents)?;
    let mut table = Table::default();
    if let Some(range) = splice_hostman_lines(&lines) {
        for line in (&lines[range]).iter().filter(|x| x.is_domain()) {
            match line {
                Line::Domain { ip, aliases } => {
                    for alias in aliases {
                        table.host_mapping.insert(alias.clone(), ip.clone());
                    }
                }
                _ => {}
            };
        }
    }

    let hostname = sys_info::hostname()?;

    let ips: Vec<_> = pnet::datalink::interfaces()
        .iter()
        .flat_map(|iface| iface.ips.iter())
        .filter(|ipnet| {
            ipnet.is_ipv4()
                && ipnet.ip() != std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))
        })
        .map(|ipnet| ipnet.ip().to_string())
        .collect();

    if ips.len() != 0 {
        table.current = Some(Current {
            host: hostname,
            ips: ips,
        });
    }

    Ok(table)
}

async fn read_hosts(file_name: &str) -> Result<Table, Box<dyn std::error::Error>> {
    use tokio::prelude::*;

    let mut contents = vec![];
    let mut file = tokio::fs::File::open(file_name).await?;

    file.read_to_end(&mut contents).await?;

    let file_contents = std::str::from_utf8(contents.as_slice())?;
    gen_table(file_contents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use combine::EasyParser;

    #[test]
    fn simple_parse_hostman_test() {
        let test: &'static str = "\
                                  # hostman:start\n\
                                  127.0.0.1       localhost\n\
                                  ::1             localhost\n\
                                  127.0.1.1       domain.local pop-os\n\
                                  # hostman:end\n\
                                  ";

        let mut lines = parse_lines().easy_parse(test).map(|x| x.0).unwrap();

        // assert_eq!(
        //     ,
        //     Ok(vec![
        //         Line::Domain {
        //             ip: "127.0.0.1".to_owned(),
        //             aliases: vec!["localhost".to_owned()]
        //         },
        //         Line::Domain {
        //             ip: "::1".to_owned(),
        //             aliases: vec!["localhost".to_owned()]
        //         },
        //         Line::Domain {
        //             ip: "127.0.1.1".to_owned(),
        //             aliases: vec!["domain.local".to_owned(), "pop-os".to_owned()]
        //         },
        //     ])
        // );
    }

    #[test]
    fn parse_hostman_test() {
        let test: &'static str = "\
                                  # hostman:start\n\
                                  127.0.0.1       localhost\n\
                                  ::1             localhost\n\
                                  127.0.1.1       domain.local pop-os\n\
                                  # hostman:end\n\
                                  ";

        assert_eq!(
            parse_hostman_lines().easy_parse(test).map(|x| x.0),
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
        );

        let test: &'static str = "\
                                  127.0.0.1       localhost\n\
                                  # hostman:start\n\
                                  127.0.0.1       localhost\n\
                                  ::1             localhost\n\
                                  127.0.1.1       domain.local pop-os\n\
                                  # hostman:end\n\
                                  127.0.0.1       localhost\n\
                                  ::1             localhost\n\
                                  127.0.1.1       domain.local pop-os\n\
                                  ";

        assert_eq!(
            parse_hostman_lines().easy_parse(test).map(|x| x.0),
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
        );
    }

    #[test]
    fn parse_domains_test() {
        let test: &'static str = "\
                                  127.0.0.1       localhost\n\
                                  ::1             localhost\n\
                                  127.0.1.1       domain.local pop-os\n\
                                  ";

        assert_eq!(
            parse_lines().easy_parse(test).map(|x| x.0),
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
            parse_lines().easy_parse(test).map(|x| x.0),
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
