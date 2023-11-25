use figlet_rs::FIGfont;

pub const VERSION: &str = "0.1.0";

pub fn print_version() {
    let standard_font = FIGfont::standard().unwrap();
    let figure = standard_font.convert("PantheonProbe");
    assert!(figure.is_some());
    println!("{}", figure.unwrap());
    println!("Pantheon Probe v{}", VERSION);
}
