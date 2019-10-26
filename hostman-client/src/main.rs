// extern crates
extern crate clap;
extern crate combine;
extern crate hostman_shared;
extern crate pnet;
extern crate reqwest;
extern crate serde;
extern crate serde_json;
extern crate sys_info;
extern crate tokio;

// modules
pub mod cli;
pub mod parsing;

// imports
use crate::parsing::{parse_lines, Line, Lines};
use hostman_shared::{Current, Table};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = cli::get_matches();
    let file_loc = matches
        .value_of(cli::HOSTS_FILE)
        .ok_or("must include hosts file")?;
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
    use combine::{
        stream::{buffered, position},
        Parser,
    };

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
