//! Минимальный PNG-энкодер без внешних зависимостей.
//!
//! Зачем свой: чтобы рендерер оставался самодостаточным (никаких `image`/`png`
//! крейтов) и собирался офлайн. Используется deflate с **хранимыми блоками**
//! (без сжатия) — файлы крупнее, зато код прост и полностью свой.

use std::io::{self, Write};

/// CRC32 (IEEE 802.3) — нужен для каждого чанка PNG.
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for b in data {
        crc ^= *b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Adler-32 — контрольная сумма потока zlib.
fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for byte in data {
        a = (a + *byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// Обернуть данные в zlib-поток с хранимыми deflate-блоками.
fn zlib_store(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 65535 * 5 + 16);
    // Заголовок zlib: метод 8, окно 32K, без словаря.
    out.push(0x78);
    out.push(0x01);
    // Разбиваем на блоки до 65535 байт.
    let mut i = 0;
    if data.is_empty() {
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
    }
    while i < data.len() {
        let end = (i + 65535).min(data.len());
        let chunk = &data[i..end];
        let last = end == data.len();
        out.push(if last { 0x01 } else { 0x00 }); // BFINAL + BTYPE=00
        let len = chunk.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(chunk);
        i = end;
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], payload: &[u8]) {
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    let mut typed = Vec::with_capacity(4 + payload.len());
    typed.extend_from_slice(kind);
    typed.extend_from_slice(payload);
    out.extend_from_slice(&typed);
    out.extend_from_slice(&crc32(&typed).to_be_bytes());
}

/// Закодировать RGB-кадр (width*height*3 байт) в PNG.
pub fn encode_rgb(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    assert_eq!(rgb.len(), (width as usize) * (height as usize) * 3, "неверный размер кадра");

    // Сырые сканлайны: фильтр 0 + пиксели строки.
    let stride = width as usize * 3;
    let mut raw = Vec::with_capacity(height as usize * (stride + 1));
    for y in 0..height as usize {
        raw.push(0);
        raw.extend_from_slice(&rgb[y * stride..(y + 1) * stride]);
    }

    let mut out = Vec::with_capacity(raw.len() + 1024);
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    // IHDR
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.push(8); // бит на канал
    ihdr.push(2); // тип цвета: truecolor RGB
    ihdr.push(0); // сжатие
    ihdr.push(0); // фильтрация
    ihdr.push(0); // без интерлейса
    chunk(&mut out, b"IHDR", &ihdr);
    // IDAT
    let compressed = zlib_store(&raw);
    chunk(&mut out, b"IDAT", &compressed);
    // IEND
    chunk(&mut out, b"IEND", &[]);
    out
}

/// Записать PNG в файл.
pub fn write_png(path: &str, width: u32, height: u32, rgb: &[u8]) -> io::Result<()> {
    let bytes = encode_rgb(width, height, rgb);
    let mut f = std::fs::File::create(path)?;
    f.write_all(&bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_known_value() {
        // CRC32 строки "123456789" = 0xCBF43926.
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn adler32_known_value() {
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn png_has_signature_and_chunks() {
        let rgb = vec![255u8, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0]; // 2x2
        let png = encode_rgb(2, 2, &rgb);
        assert_eq!(&png[0..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        // IHDR присутствует.
        assert!(png.windows(4).any(|w| w == b"IHDR"));
        assert!(png.windows(4).any(|w| w == b"IDAT"));
        assert!(png.windows(4).any(|w| w == b"IEND"));
    }

    #[test]
    fn zlib_store_roundtrips_length() {
        let data = vec![7u8; 1000];
        let z = zlib_store(&data);
        // Заголовок 2 + блок(1+2+2) + данные + adler(4).
        assert_eq!(z.len(), 2 + 5 + data.len() + 4);
        assert_eq!(z[0], 0x78);
    }
}
