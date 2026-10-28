#[path = "../src/tables.rs"]
mod tables;

fn main() {
    for k in 0..=19 {
        println!("power,64,{k},{}", tables::pow10_64(k));
    }
    for k in 1..=19 {
        let entry = tables::recip_64(k);
        println!("reciprocal,64,{k},{},{}", entry.m, entry.post);
    }
    for k in 0..=38 {
        println!("power,128,{k},{}", tables::pow10(k));
    }
    for k in 1..=38 {
        let entry = tables::recip(k);
        println!("reciprocal,128,{k},{},{}", entry.m, entry.post);
    }
}
