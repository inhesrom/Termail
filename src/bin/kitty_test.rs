/// Minimal Kitty graphics protocol test for Ghostty.
/// Sends a small red square directly via escape sequences, bypassing ratatui.
///
/// Run: cargo run --bin kitty_test
use std::io::{self, Write};

fn main() {
    // Create a 4x4 RGBA image (solid red)
    let w: u32 = 4;
    let h: u32 = 4;
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for _ in 0..(w * h) {
        pixels.extend_from_slice(&[255, 0, 0, 255]); // RGBA red
    }

    // Base64 encode
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&pixels);

    let id: u32 = 1;
    let [id_extra, id_r, id_g, id_b] = id.to_be_bytes();

    let stdout = io::stdout();
    let mut out = stdout.lock();

    println!("=== Kitty Graphics Protocol Test ===");
    println!("Transmitting {}x{} red image (id={})...", w, h, id);
    println!();

    // Transmit image with virtual placement (U=1)
    write!(
        out,
        "\x1b_Gq=2,i={},a=T,U=1,f=32,t=d,s={},v={},m=0;{}\x1b\\",
        id, w, h, b64
    )
    .unwrap();
    out.flush().unwrap();

    println!("Transmit done. Now placing unicode placeholders:");
    println!();

    // Row/column diacritics from Kitty spec
    let diacritics: [char; 10] = [
        '\u{305}', '\u{30D}', '\u{30E}', '\u{310}', '\u{312}',
        '\u{33D}', '\u{33E}', '\u{33F}', '\u{346}', '\u{34A}',
    ];

    // Set foreground color to encode image ID (lower 24 bits)
    write!(out, "\x1b[38;2;{};{};{}m", id_r, id_g, id_b).unwrap();

    // Write 4x4 grid of placeholders with explicit per-cell diacritics
    for row in 0..h {
        for col in 0..w {
            write!(
                out,
                "\u{10EEEE}{}{}{}",
                diacritics[row as usize],     // row diacritic
                diacritics[col as usize],     // column diacritic
                diacritics[id_extra as usize] // id_extra diacritic
            )
            .unwrap();
        }
        writeln!(out).unwrap();
    }

    // Reset colors
    write!(out, "\x1b[m").unwrap();
    out.flush().unwrap();

    println!();
    println!("If you see a red square above, Kitty protocol works!");
    println!();

    // Also test: placeholders with INHERITED diacritics (bare U+10EEEE)
    println!("Now testing inherited diacritics (bare placeholders):");
    println!();

    // Set foreground color again
    write!(out, "\x1b[38;2;{};{};{}m", id_r, id_g, id_b).unwrap();

    for row in 0..h {
        // First placeholder has explicit diacritics
        write!(
            out,
            "\u{10EEEE}{}{}{}",
            diacritics[row as usize],
            diacritics[0],
            diacritics[id_extra as usize]
        )
        .unwrap();
        // Remaining placeholders are bare (inherit diacritics)
        for _ in 1..w {
            write!(out, "\u{10EEEE}").unwrap();
        }
        writeln!(out).unwrap();
    }

    write!(out, "\x1b[m").unwrap();
    out.flush().unwrap();

    println!();
    println!("If you see a SECOND red square above, inherited diacritics work too!");
    println!("Press Enter to exit...");

    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
}
