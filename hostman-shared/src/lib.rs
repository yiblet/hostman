extern crate serde;
extern crate serde_json;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

type Host = String;
type Ip = String;

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Table {
    pub host_mapping: BTreeMap<Host, Ip>,
    pub current: Option<Current>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Current {
    pub host: Host,
    pub ips: Vec<Ip>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
