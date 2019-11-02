use clap::{App, Arg, ArgMatches};

pub const HOSTS_FILE: &'static str = "HOSTS_FILE";
pub const NETWORK: &'static str = "NETWORK";

pub fn get_matches<'a>() -> ArgMatches<'a> {
    App::new("Hostman Client")
        .version("0.1")
        .author("Shalom Yiblet <shalom.yiblet@gmail.com>")
        .arg(
            Arg::with_name(HOSTS_FILE)
                .help("Sets the hosts file to monitor and use")
                .required(true)
                .takes_value(true)
                .index(1),
        )
        .arg(
            Arg::with_name(NETWORK)
                .long("network")
                .short("n")
                .help("sets up the ip network mask to upload to etc hosts")
                .takes_value(true),
        )
        .get_matches()
}
