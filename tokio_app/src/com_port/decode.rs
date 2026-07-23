//! Декодирование 20-байтных кадров протокола РКМ / РКМ-С.
//!
//! Оба варианта устройства используют одну и ту же раскладку каналов и формулы
//! `to_int` / `to_signed_int` / `convert`. У РКМ-С бит 7 всегда 0 (у базового РКМ — 1),
//! на декодирование это не влияет: маска `0x7E` бит 7 не учитывает.

use super::model::Frame;

/// 12-битное беззнаковое значение из пары байт (значащие биты 6..1, маска `0x7E`).
pub fn to_int(high: u8, low: u8) -> i32 {
    (((high & 0x7E) as i32) << 5) + (((low & 0x7E) as i32) >> 1)
}

/// Знаковое 12-битное значение (доп. код, знаковый бит — бит 11).
pub fn to_signed_int(high: u8, low: u8) -> i32 {
    let n = to_int(high, low);
    if n & 0x0800 != 0 {
        n | !0x0FFF_i32
    } else {
        n
    }
}

/// Преобразование канала: биполярный — `-to_signed_int`, униполярный — `to_int`.
pub fn convert(high: u8, low: u8, bipolar: bool) -> i32 {
    if bipolar {
        -to_signed_int(high, low)
    } else {
        to_int(high, low)
    }
}

/// Декодирует кадр РКМ / РКМ-С.
///
/// Раскладка как Java `RcmInVariable`:
/// - 0–1: РЕО-1 (биполярный, с инверсией для отображения)
/// - 2–3: BASE-1
/// - 4–5: РЕО-2
/// - 6–7: ЭКГ
/// - 8–9: BASE-2
pub fn decode_frame(raw: &[u8; 20]) -> Frame {
    Frame::new(
        convert(raw[0], raw[1], true),
        convert(raw[2], raw[3], false),
        convert(raw[6], raw[7], true),
        convert(raw[8], raw[9], false),
        convert(raw[4], raw[5], true),
    )
}

/// Декодирует кадр РКМ-С. По составу каналов идентичен базовому РКМ → тот же `Frame`.
pub fn decode_frame_rcms(raw: &[u8; 20]) -> Frame {
    decode_frame(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_int_extracts_12_bits() {
        // high=0b0_111111_0 (0x7E), low=0b0_111111_0 (0x7E) → 0xFFF
        assert_eq!(to_int(0x7E, 0x7E), 0x0FFF);
        assert_eq!(to_int(0x00, 0x00), 0);
        // бит 7 и бит 0 не участвуют
        assert_eq!(to_int(0xFE, 0xFF), to_int(0x7E, 0x7E));
    }

    #[test]
    fn bipolar_negates_signed() {
        // положительное сырое → отрицательный bipolar
        let raw_pos = to_int(0x02, 0x00); // небольшой положительный
        assert!(raw_pos > 0);
        assert_eq!(convert(0x02, 0x00, true), -raw_pos);
    }

    #[test]
    fn decode_skips_unused_slots() {
        let mut raw = [0u8; 20];
        // РЕО-1
        raw[0] = 0x02;
        raw[1] = 0x04;
        // BASE-1
        raw[2] = 0x03;
        raw[3] = 0x06;
        // РЕО-2 (пара 2)
        raw[4] = 0x09;
        raw[5] = 0x0C;
        // ЭКГ
        raw[6] = 0x05;
        raw[7] = 0x08;
        // BASE-2
        raw[8] = 0x07;
        raw[9] = 0x0A;
        // остальные пары — мусор
        for i in 5..10 {
            raw[i * 2] = 0x7E;
            raw[i * 2 + 1] = 0x7E;
        }
        raw[18] = 0x01;
        raw[19] = 0x00;

        let frame = decode_frame(&raw);
        assert_eq!(frame.rheo1, convert(0x02, 0x04, true));
        assert_eq!(frame.base1, convert(0x03, 0x06, false));
        assert_eq!(frame.rheo2, convert(0x09, 0x0C, true));
        assert_eq!(frame.ecg, convert(0x05, 0x08, true));
        assert_eq!(frame.base2, convert(0x07, 0x0A, false));

        let rcms = decode_frame_rcms(&raw);
        assert_eq!(frame.rheo1, rcms.rheo1);
        assert_eq!(frame.base1, rcms.base1);
        assert_eq!(frame.ecg, rcms.ecg);
        assert_eq!(frame.base2, rcms.base2);
        assert_eq!(frame.rheo2, rcms.rheo2);
    }
}
