use clap::{App, Arg, ArgMatches};

pub fn get_matches<'a>() -> ArgMatches<'a> {
    App::new("Hostman Server")
        .version("0.1")
        .author("Shalom Yiblet <shalom.yiblet@gmail.com>")
        .arg(
            Arg::with_name("LOCATION")
                .help("Sets the input file to use")
                .index(1),
        )
        .arg(
            Arg::with_name("port")
                .help("sets the port")
                .long("port")
                .short("p")
                .default_value("15332")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("host")
                .help("sets the host")
                .long("host")
                .short("h")
                .default_value("0.0.0.0")
                .takes_value(true),
        )
        .get_matches()
}
