use sysinfo::Components;
fn main() {
    let mut components = Components::new_with_refreshed_list();
    components.refresh();
    for comp in components.iter() {
        println!("{:?} {}", comp.label(), comp.temperature());
    }
}
