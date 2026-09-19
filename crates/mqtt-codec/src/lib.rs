pub const MAX_VARIABLE_BYTE_INTEGER: u32 = 268_435_455;

#[derive(Debug, PartialEq)]
pub enum EncodeError {
    OutOfRange,
}

pub fn encode_variable_byte_integer(value: u32) -> Result<Vec<u8>, EncodeError> {
    if value > MAX_VARIABLE_BYTE_INTEGER {
        return Err(EncodeError::OutOfRange);
    }

    let mut remaining_value = value;
    let mut encoded = Vec::with_capacity(4);

    loop {
        let mut byte = (remaining_value % 128) as u8;
        remaining_value /= 128;
        if remaining_value > 0 {
            byte |= 0b1000_0000;
        }
        encoded.push(byte);
        if remaining_value == 0 {
            break;
        }
    }
    Ok(encoded)
}

#[derive(Debug, PartialEq)]
pub enum DecodeError {
    Incomplete,
    Malformed,
    PacketTooLarge,
}

pub fn decode_frame_length(input: &[u8], max_packet_size: usize) -> Result<usize, DecodeError> {
    if input.is_empty() {
        return Err(DecodeError::Incomplete);
    }

    let (remaining_length, encoded_length) = decode_variable_byte_integer(&input[1..])?;

    let remaining_length =
        usize::try_from(remaining_length).map_err(|_| DecodeError::PacketTooLarge)?;

    let frame_length = 1_usize
        .checked_add(encoded_length)
        .and_then(|length| length.checked_add(remaining_length))
        .ok_or(DecodeError::PacketTooLarge)?;

    if frame_length > max_packet_size {
        Err(DecodeError::PacketTooLarge)
    } else {
        Ok(frame_length)
    }
}

pub fn decode_variable_byte_integer(input: &[u8]) -> Result<(u32, usize), DecodeError> {
    let mut value = 0_u32;
    let mut multiplier = 1_u32;

    for (index, &byte) in input.iter().take(4).enumerate() {
        let digit = u32::from(byte & 0b0111_1111);
        value += digit * multiplier;

        if !has_more_bytes(byte) {
            if index > 0 && digit == 0 {
                return Err(DecodeError::Malformed);
            }
            return Ok((value, index + 1));
        }

        if index == 3 {
            return Err(DecodeError::Malformed);
        }

        multiplier *= 128;
    }
    Err(DecodeError::Incomplete)
}

fn has_more_bytes(byte: u8) -> bool {
    byte & 0b1000_0000 != 0
}

pub struct FrameDecoder {
    buffer: Vec<u8>,
    max_packet_size: usize,
}

impl FrameDecoder {
    pub fn new(max_packet_size: usize) -> Self {
        Self {
            buffer: Vec::new(),
            max_packet_size,
        }
    }

    pub fn push(&mut self, input: &[u8]) {
        self.buffer.extend_from_slice(input);
    }

    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, DecodeError> {
        let frame_length = match decode_frame_length(&self.buffer, self.max_packet_size) {
            Ok(length) => length,
            Err(DecodeError::Incomplete) => return Ok(None),
            Err(error) => return Err(error),
        };

        if self.buffer.len() < frame_length {
            return Ok(None);
        }

        let frame = self.buffer.drain(..frame_length).collect();
        Ok(Some(frame))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_encoded() {
        assert_eq!(encode_variable_byte_integer(0), Ok(vec![0x00]));
        assert_eq!(encode_variable_byte_integer(127), Ok(vec![0x7f]));
        assert_eq!(encode_variable_byte_integer(128), Ok(vec![0x80, 0x01]));
        assert_eq!(
            encode_variable_byte_integer(MAX_VARIABLE_BYTE_INTEGER + 1),
            Err(EncodeError::OutOfRange)
        );
    }

    #[test]
    fn has_more_bytes_works() {
        assert!(!has_more_bytes(0x00));
        assert!(!has_more_bytes(0x7f));
        assert!(has_more_bytes(0x80));
        assert!(has_more_bytes(0xff));
    }

    #[test]
    fn empty_input_is_incomplete() {
        assert_eq!(
            decode_variable_byte_integer(&[]),
            Err(DecodeError::Incomplete)
        );
    }

    #[test]
    fn one_byte_values_are_decoded() {
        assert_eq!(decode_variable_byte_integer(&[0x00]), Ok((0, 1)));
        assert_eq!(decode_variable_byte_integer(&[0x7f]), Ok((127, 1)));
    }

    #[test]
    fn two_byte_values_are_decoded() {
        assert_eq!(decode_variable_byte_integer(&[0x80, 0x01]), Ok((128, 2)));
        assert_eq!(decode_variable_byte_integer(&[0xff, 0x7f]), Ok((16_383, 2)));
    }

    #[test]
    fn input_is_incomplete_or_malformed() {
        assert_eq!(
            decode_variable_byte_integer(&[0x80]),
            Err(DecodeError::Incomplete)
        );
        assert_eq!(
            decode_variable_byte_integer(&[0x80; 4]),
            Err(DecodeError::Malformed)
        );
    }

    #[test]
    fn maximum_value_is_decoded() {
        assert_eq!(
            decode_variable_byte_integer(&[0xff, 0xff, 0xff, 0x7f]),
            Ok((MAX_VARIABLE_BYTE_INTEGER, 4))
        );
    }

    #[test]
    fn non_minimal_encoding_is_malformed() {
        assert_eq!(
            decode_variable_byte_integer(&[0x80, 0x00]),
            Err(DecodeError::Malformed)
        );

        assert_eq!(
            decode_variable_byte_integer(&[0xff, 0x00]),
            Err(DecodeError::Malformed)
        );
    }
    #[test]
    fn variable_byte_integers_round_trip() {
        for value in [
            0,
            1,
            127,
            128,
            16_383,
            16_384,
            2_097_151,
            2_097_152,
            MAX_VARIABLE_BYTE_INTEGER,
        ] {
            let encoded = encode_variable_byte_integer(value).unwrap();

            assert_eq!(
                decode_variable_byte_integer(&encoded),
                Ok((value, encoded.len()))
            );
        }
    }

    #[test]
    fn frame_length_is_calculated_from_header() {
        assert_eq!(decode_frame_length(&[0xc0, 0x00], 1024), Ok(2));
        assert_eq!(decode_frame_length(&[0x30, 0x03], 1024), Ok(5));
    }

    #[test]
    fn incomplete_frame_header_is_reported() {
        assert_eq!(decode_frame_length(&[], 1024), Err(DecodeError::Incomplete));
        assert_eq!(
            decode_frame_length(&[0x30], 1024),
            Err(DecodeError::Incomplete)
        );
    }
    #[test]
    fn oversized_frame_is_rejected() {
        assert_eq!(
            decode_frame_length(&[0x30, 0x03], 4),
            Err(DecodeError::PacketTooLarge)
        );
    }
    #[test]
    fn multi_byte_remaining_length_is_included_in_frame_length() {
        assert_eq!(decode_frame_length(&[0x30, 0x80, 0x01], 1024), Ok(131));

        assert_eq!(
            decode_frame_length(&[0x30, 0x80, 0x01], 130),
            Err(DecodeError::PacketTooLarge)
        );
    }

    #[test]
    fn fragmented_frame_waits_for_remaining_bytes() {
        let mut decoder = FrameDecoder::new(1024);

        decoder.push(&[0xc0]);
        assert_eq!(decoder.next_frame(), Ok(None));

        decoder.push(&[0x00]);
        assert_eq!(decoder.next_frame(), Ok(Some(vec![0xc0, 0x00])));
    }

    #[test]
    fn coalesced_frames_are_decoded_individually() {
        let mut decoder = FrameDecoder::new(1024);

        decoder.push(&[0xc0, 0x00, 0xc0, 0x00]);
        assert_eq!(decoder.next_frame(), Ok(Some(vec![0xc0, 0x00])));
        assert_eq!(decoder.next_frame(), Ok(Some(vec![0xc0, 0x00])));
        assert_eq!(decoder.next_frame(), Ok(None));
    }
}
