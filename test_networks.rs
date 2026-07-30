use sysinfo::Networks;
fn main() {
    let mut networks = Networks::new_with_refreshed_list();
    networks.refresh();
    for (name, net) in networks.iter() {
        println!("{}: total_rx={}, rx={}", name, net.total_received(), net.received());
    }
}
