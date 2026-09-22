pub const MAX_VARIABLE_BYTE_INTEGER: u32 = 268_435_455;

#[derive(Debug, PartialEq)]
pub enum EncodeError {
    OutOfRange,
}

#[derive(Debug, PartialEq)]
pub enum Packet {
    Connect(Connect),
    Disconnect,
    PingReq,
}

pub fn decode_packet(input: &[u8]) -> Result<Packet, DecodeError> {
    let first_byte = match input.first() {
        None => return Err(DecodeError::Incomplete),
        Some(first_byte) => first_byte,
    };

    let packet_type = decode_packet_type(*first_byte)?;
    match packet_type {
        PacketType::Connect => {
            let connect = decode_connect(input)?;
            Ok(Packet::Connect(connect))
        }
        PacketType::PingReq => {
            decode_pingreq(input)?;
            Ok(Packet::PingReq)
        }
        PacketType::Disconnect => {
            decode_disconnect(input)?;
            Ok(Packet::Disconnect)
        }
    }
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
    Unsupported,
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

#[derive(Debug, PartialEq)]
pub enum PacketType {
    Connect,
    Disconnect,
    PingReq,
}

pub fn decode_packet_type(byte: u8) -> Result<PacketType, DecodeError> {
    match byte >> 4 {
        1 if byte & 0b0000_1111 == 0 => Ok(PacketType::Connect),
        12 if byte & 0b0000_1111 == 0 => Ok(PacketType::PingReq),
        14 if byte & 0b0000_1111 == 0 => Ok(PacketType::Disconnect),
        _ => Err(DecodeError::Malformed),
    }
}

pub fn decode_utf8_string(input: &[u8]) -> Result<(&str, usize), DecodeError> {
    let (bytes, end) = decode_binary_data(input)?;

    let value = match std::str::from_utf8(bytes) {
        Ok(value) => value,
        Err(_) => return Err(DecodeError::Malformed),
    };

    if value.contains('\0') {
        return Err(DecodeError::Malformed);
    }

    Ok((value, end))
}

pub fn decode_binary_data(input: &[u8]) -> Result<(&[u8], usize), DecodeError> {
    if input.len() < 2 {
        return Err(DecodeError::Incomplete);
    }

    let length = usize::from(u16::from_be_bytes([input[0], input[1]]));
    let end = 2 + length;

    if input.len() < end {
        return Err(DecodeError::Incomplete);
    }

    Ok((&input[2..end], end))
}

pub fn decode_u16(input: &[u8]) -> Result<(u16, usize), DecodeError> {
    if input.len() < 2 {
        return Err(DecodeError::Incomplete);
    }

    let value = u16::from_be_bytes([input[0], input[1]]);
    Ok((value, 2))
}

pub fn decode_u32(input: &[u8]) -> Result<(u32, usize), DecodeError> {
    if input.len() < 4 {
        return Err(DecodeError::Incomplete);
    }

    let value = u32::from_be_bytes([input[0], input[1], input[2], input[3]]);
    Ok((value, 4))
}

#[derive(Debug, PartialEq)]
pub struct Connect {
    pub client_id: String,
    pub clean_start: bool,
    pub keep_alive: u16,
    pub request_problem_information: bool,
    pub maximum_packet_size: u32,
}

pub fn decode_connect(input: &[u8]) -> Result<Connect, DecodeError> {
    let frame_length = decode_frame_length(input, usize::MAX)?;

    if input.len() < frame_length {
        return Err(DecodeError::Incomplete);
    }

    if input.len() > frame_length {
        return Err(DecodeError::Malformed);
    }

    decode_packet_type(input[0])?;

    let (_, remaining_length_bytes) = decode_variable_byte_integer(&input[1..])?;
    let body_start = 1 + remaining_length_bytes;
    let body = &input[body_start..frame_length];

    let (protocol_name, protocol_name_bytes) = decode_utf8_string(body)?;

    if protocol_name != "MQTT" {
        return Err(DecodeError::Malformed);
    }

    let mut position = protocol_name_bytes;

    let protocol_version = *body.get(position).ok_or(DecodeError::Incomplete)?;
    position += 1;

    if protocol_version != 5 {
        return Err(DecodeError::Malformed);
    }

    let connect_flags = *body.get(position).ok_or(DecodeError::Incomplete)?;
    if connect_flags & 0b0000_0001 != 0 {
        return Err(DecodeError::Malformed);
    }
    let will_requested = connect_flags & 0b0000_0100 != 0;
    let will_options = connect_flags & 0b0011_1000;

    if !will_requested && will_options != 0 {
        return Err(DecodeError::Malformed);
    }

    let will_qos = (connect_flags & 0b0001_1000) >> 3;
    if will_qos == 3 {
        return Err(DecodeError::Malformed);
    }

    if connect_flags & 0b1100_0100 != 0 {
        return Err(DecodeError::Unsupported);
    }

    position += 1;

    let clean_start = connect_flags & 0b0000_0010 != 0;

    let (keep_alive, keep_alive_bytes) = decode_u16(&body[position..])?;
    position += keep_alive_bytes;

    let (property_length, property_length_bytes) = decode_variable_byte_integer(&body[position..])?;
    position += property_length_bytes;

    let property_length =
        usize::try_from(property_length).map_err(|_| DecodeError::PacketTooLarge)?;
    let property_end = position
        .checked_add(property_length)
        .ok_or(DecodeError::PacketTooLarge)?;
    let properties = body
        .get(position..property_end)
        .ok_or(DecodeError::Malformed)?;

    let mut request_problem_information = true;
    let mut request_problem_information_seen = false;
    let mut maximum_packet_size = u32::MAX;
    let mut maximum_packet_size_seen = false;
    let mut property_position = 0;

    while property_position < properties.len() {
        let (identifier, identifier_bytes) =
            decode_variable_byte_integer(&properties[property_position..])
                .map_err(|_| DecodeError::Malformed)?;
        property_position += identifier_bytes;

        match identifier {
            0x17 => {
                if request_problem_information_seen {
                    return Err(DecodeError::Malformed);
                }
                let value = properties
                    .get(property_position)
                    .ok_or(DecodeError::Malformed)?;
                match *value {
                    0 => request_problem_information = false,
                    1 => request_problem_information = true,
                    _ => return Err(DecodeError::Malformed),
                }
                request_problem_information_seen = true;
                property_position += 1;
            }
            0x23 => return Err(DecodeError::Malformed),
            0x27 => {
                if maximum_packet_size_seen {
                    return Err(DecodeError::Malformed);
                }
                let (packet_size, packet_size_bytes) = decode_u32(&properties[property_position..])
                    .map_err(|_| DecodeError::Malformed)?;
                if packet_size == 0 {
                    return Err(DecodeError::Malformed);
                }
                maximum_packet_size = packet_size;
                property_position += packet_size_bytes;
                maximum_packet_size_seen = true;
            }
            _ => return Err(DecodeError::Unsupported),
        }
    }

    position = property_end;

    let (client_id, client_id_bytes) = match decode_utf8_string(&body[position..]) {
        Ok(value) => value,
        Err(DecodeError::Incomplete) => return Err(DecodeError::Malformed),
        Err(error) => return Err(error),
    };

    position += client_id_bytes;

    if position != body.len() {
        return Err(DecodeError::Malformed);
    }

    Ok(Connect {
        client_id: client_id.to_owned(),
        clean_start,
        keep_alive,
        request_problem_information,
        maximum_packet_size,
    })
}

pub fn decode_pingreq(input: &[u8]) -> Result<(), DecodeError> {
    let frame_length = decode_frame_length(input, usize::MAX)?;
    if frame_length != 2 {
        return Err(DecodeError::Malformed);
    }

    if input.len() < frame_length {
        return Err(DecodeError::Incomplete);
    }

    if input.len() > frame_length {
        return Err(DecodeError::Malformed);
    }

    let packet_type = decode_packet_type(input[0])?;
    if packet_type != PacketType::PingReq {
        return Err(DecodeError::Malformed);
    }
    Ok(())
}

pub fn decode_disconnect(input: &[u8]) -> Result<(), DecodeError> {
    let frame_length = decode_frame_length(input, usize::MAX)?;
    if input.len() < frame_length {
        return Err(DecodeError::Incomplete);
    }

    if input.len() > frame_length {
        return Err(DecodeError::Malformed);
    }

    if frame_length != 2 {
        return Err(DecodeError::Malformed);
    }

    let packet_type = decode_packet_type(input[0])?;
    if packet_type != PacketType::Disconnect {
        return Err(DecodeError::Malformed);
    }
    Ok(())
}

pub fn encode_connack() -> Vec<u8> {
    vec![
        0x20, 0x0d, 0x00, 0x00, 0x0a, 0x24, 0x00, 0x25, 0x00, 0x28, 0x01, 0x29, 0x00, 0x2a, 0x00,
    ]
}

pub fn encode_pingresp() -> Vec<u8> {
    vec![0xd0, 0x00]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_encoded() {
        assert_eq!(
            encode_variable_byte_integer(0),
            Ok(vec![0x00]) // Value 0, continuation bit clear
        );
        assert_eq!(
            encode_variable_byte_integer(127),
            Ok(vec![0x7f]) // Value 127, continuation bit clear
        );
        assert_eq!(
            encode_variable_byte_integer(128),
            Ok(vec![0x80, 0x01]) // Digits 0 and 1 in base 128
        );
        assert_eq!(
            encode_variable_byte_integer(MAX_VARIABLE_BYTE_INTEGER + 1),
            Err(EncodeError::OutOfRange)
        );
    }

    #[test]
    fn has_more_bytes_works() {
        assert!(!has_more_bytes(0x00)); // Continuation bit clear
        assert!(!has_more_bytes(0x7f)); // Continuation bit clear
        assert!(has_more_bytes(0x80)); // Continuation bit set, value bits clear
        assert!(has_more_bytes(0xff)); // Continuation and all value bits set
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
        assert_eq!(decode_variable_byte_integer(&[0x00]), Ok((0, 1))); // Value 0
        assert_eq!(decode_variable_byte_integer(&[0x7f]), Ok((127, 1))); // Value 127
    }

    #[test]
    fn two_byte_values_are_decoded() {
        assert_eq!(decode_variable_byte_integer(&[0x80, 0x01]), Ok((128, 2))); // Digits 0, 1
        assert_eq!(decode_variable_byte_integer(&[0xff, 0x7f]), Ok((16_383, 2))); // Digits 127, 127
    }

    #[test]
    fn input_is_incomplete_or_malformed() {
        assert_eq!(
            decode_variable_byte_integer(&[0x80]), // Continuation bit set, next byte missing
            Err(DecodeError::Incomplete)
        );
        assert_eq!(
            decode_variable_byte_integer(&[0x80; 4]), // Four continuation bytes, no terminator
            Err(DecodeError::Malformed)
        );
    }

    #[test]
    fn maximum_value_is_decoded() {
        assert_eq!(
            decode_variable_byte_integer(&[0xff, 0xff, 0xff, 0x7f]), // Four base-128 digits, all 127
            Ok((MAX_VARIABLE_BYTE_INTEGER, 4))
        );
    }

    #[test]
    fn non_minimal_encoding_is_malformed() {
        assert_eq!(
            decode_variable_byte_integer(&[0x80, 0x00]), // Value 0 encoded in two bytes
            Err(DecodeError::Malformed)
        );

        assert_eq!(
            decode_variable_byte_integer(&[0xff, 0x00]), // Value 127 encoded in two bytes
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
        assert_eq!(decode_frame_length(&[0xc0, 0x00], 1024), Ok(2)); // PINGREQ, empty body
        assert_eq!(decode_frame_length(&[0x30, 0x03], 1024), Ok(5)); // PUBLISH, 3-byte body
    }

    #[test]
    fn incomplete_frame_header_is_reported() {
        assert_eq!(decode_frame_length(&[], 1024), Err(DecodeError::Incomplete));
        assert_eq!(
            decode_frame_length(&[0x30], 1024), // PUBLISH header without Remaining Length
            Err(DecodeError::Incomplete)
        );
    }
    #[test]
    fn oversized_frame_is_rejected() {
        assert_eq!(
            decode_frame_length(&[0x30, 0x03], 4), // PUBLISH with 3-byte body totals 5 bytes
            Err(DecodeError::PacketTooLarge)
        );
    }
    #[test]
    fn multi_byte_remaining_length_is_included_in_frame_length() {
        // PUBLISH with Remaining Length 128: 1 fixed-header byte + 2 length bytes + 128 body bytes.
        assert_eq!(decode_frame_length(&[0x30, 0x80, 0x01], 1024), Ok(131));

        assert_eq!(
            decode_frame_length(&[0x30, 0x80, 0x01], 130),
            Err(DecodeError::PacketTooLarge)
        );
    }

    #[test]
    fn fragmented_frame_waits_for_remaining_bytes() {
        let mut decoder = FrameDecoder::new(1024);

        decoder.push(&[0xc0]); // PINGREQ fixed header
        assert_eq!(decoder.next_frame(), Ok(None));

        decoder.push(&[0x00]); // Remaining Length: 0
        assert_eq!(decoder.next_frame(), Ok(Some(vec![0xc0, 0x00])));
    }

    #[test]
    fn coalesced_frames_are_decoded_individually() {
        let mut decoder = FrameDecoder::new(1024);

        decoder.push(&[
            0xc0, 0x00, // First PINGREQ with empty body
            0xc0, 0x00, // Second PINGREQ with empty body
        ]);
        assert_eq!(decoder.next_frame(), Ok(Some(vec![0xc0, 0x00])));
        assert_eq!(decoder.next_frame(), Ok(Some(vec![0xc0, 0x00])));
        assert_eq!(decoder.next_frame(), Ok(None));
    }

    #[test]
    fn fragmented_body_waits_for_remaining_bytes() {
        let mut decoder = FrameDecoder::new(1024);
        decoder.push(&[
            0x30, 0x03, // PUBLISH, Remaining Length 3
            0xaa, // First body byte
        ]);
        assert_eq!(decoder.next_frame(), Ok(None));
        decoder.push(&[0xbb, 0xcc]); // Remaining body bytes
        assert_eq!(
            decoder.next_frame(),
            Ok(Some(vec![0x30, 0x03, 0xaa, 0xbb, 0xcc]))
        );
    }

    #[test]
    fn oversized_frame_is_rejected_before_body_arrives() {
        let mut decoder = FrameDecoder::new(4);
        decoder.push(&[0x30, 0x03]); // PUBLISH with 3-byte body totals 5 bytes
        assert_eq!(decoder.next_frame(), Err(DecodeError::PacketTooLarge));
    }

    #[test]
    fn connect_packet_type_is_decoded() {
        assert_eq!(decode_packet_type(0x10), Ok(PacketType::Connect)); // CONNECT, flags 0
    }

    #[test]
    fn connect_with_non_zero_flags_is_malformed() {
        assert_eq!(decode_packet_type(0x11), Err(DecodeError::Malformed)); // CONNECT, reserved flag set
    }

    #[test]
    fn utf8_string_is_decoded() {
        assert_eq!(
            decode_utf8_string(&[
                0x00, 0x04, // String length: 4
                b'M', b'Q', b'T', b'T', // UTF-8 string: "MQTT"
            ]),
            Ok(("MQTT", 6))
        );
    }

    #[test]
    fn incomplete_utf8_string_is_reported() {
        assert_eq!(
            decode_utf8_string(&[0x00]), // First byte of the two-byte length prefix
            Err(DecodeError::Incomplete)
        );

        assert_eq!(
            decode_utf8_string(&[
                0x00, 0x04, // String length: 4
                b'M', b'Q', // Only 2 of 4 string bytes
            ]),
            Err(DecodeError::Incomplete)
        );
    }

    #[test]
    fn invalid_utf8_string_is_malformed() {
        assert_eq!(
            decode_utf8_string(&[
                0x00, 0x02, // String length: 2
                0xc3, 0x28, // Invalid UTF-8 sequence
            ]),
            Err(DecodeError::Malformed)
        );
    }

    #[test]
    fn null_character_in_utf8_string_is_malformed() {
        assert_eq!(
            decode_utf8_string(&[
                0x00, 0x01, // String length: 1
                0x00, // Forbidden null character
            ]),
            Err(DecodeError::Malformed)
        );
    }

    #[test]
    fn binary_data_is_decoded() {
        assert_eq!(
            decode_binary_data(&[
                0x00, 0x03, // Binary data length: 3
                0x00, 0xff, 0x80, // Binary data
            ]),
            Ok((&[0x00, 0xff, 0x80][..], 5))
        );
    }

    #[test]
    fn two_byte_integer_is_decoded() {
        assert_eq!(decode_u16(&[0x00, 0x3c]), Ok((60, 2))); // Big-endian value 60
    }

    #[test]
    fn incomplete_two_byte_integer_is_reported() {
        assert_eq!(decode_u16(&[]), Err(DecodeError::Incomplete)); // No bytes
        assert_eq!(decode_u16(&[0x00]), Err(DecodeError::Incomplete)); // Only the high byte
    }

    #[test]
    fn minimal_connect_packet_is_decoded() {
        let frame = [
            0x10, 0x10, // CONNECT, Remaining Length 16
            0x00, 0x04, b'M', b'Q', b'T', b'T', 0x05, // MQTT version 5
            0x02, // Clean Start
            0x00, 0x3c, // Keep Alive: 60
            0x00, // No properties
            0x00, 0x03, b'a', b'b', b'c', // Client ID: "abc"
        ];

        assert_eq!(
            decode_connect(&frame),
            Ok(Connect {
                client_id: "abc".to_owned(),
                clean_start: true,
                keep_alive: 60,
                request_problem_information: true,
                maximum_packet_size: u32::MAX,
            })
        );
    }

    #[test]
    fn connect_reserved_flag_is_malformed() {
        let frame = [
            0x10, 0x10, // CONNECT, Remaining Length 16
            0x00, 0x04, b'M', b'Q', b'T', b'T', 0x05, // MQTT version 5
            0x03, // Clean Start plus reserved bit set
            0x00, 0x3c, // Keep Alive: 60
            0x00, // No properties
            0x00, 0x03, b'a', b'b', b'c', // Client ID: "abc"
        ];

        assert_eq!(decode_connect(&frame), Err(DecodeError::Malformed));
    }

    #[test]
    fn connect_with_will_is_unsupported() {
        let frame = [
            0x10, 0x10, // CONNECT, Remaining Length 16
            0x00, 0x04, b'M', b'Q', b'T', b'T', 0x05, // MQTT version 5
            0x06, // Clean Start and Will Flag
            0x00, 0x3c, // Keep Alive: 60
            0x00, // No properties
            0x00, 0x03, b'a', b'b', b'c', // Client ID: "abc"
        ];

        assert_eq!(decode_connect(&frame), Err(DecodeError::Unsupported));
    }

    #[test]
    fn connect_with_username_is_unsupported() {
        let frame = [
            0x10, 0x10, // CONNECT, Remaining Length 16
            0x00, 0x04, b'M', b'Q', b'T', b'T', 0x05, // MQTT version 5
            0x82, // Username Flag and Clean Start
            0x00, 0x3c, // Keep Alive: 60
            0x00, // No properties
            0x00, 0x03, b'a', b'b', b'c', // Client ID: "abc"
        ];

        assert_eq!(decode_connect(&frame), Err(DecodeError::Unsupported));
    }

    #[test]
    fn connect_with_password_is_unsupported() {
        let frame = [
            0x10, 0x10, // CONNECT, Remaining Length 16
            0x00, 0x04, b'M', b'Q', b'T', b'T', 0x05, // MQTT version 5
            0x42, // Password Flag and Clean Start
            0x00, 0x3c, // Keep Alive: 60
            0x00, // No properties
            0x00, 0x03, b'a', b'b', b'c', // Client ID: "abc"
        ];

        assert_eq!(decode_connect(&frame), Err(DecodeError::Unsupported));
    }

    #[test]
    fn will_qos_without_will_is_malformed() {
        let frame = [
            0x10, 0x10, // CONNECT, Remaining Length 16
            0x00, 0x04, b'M', b'Q', b'T', b'T', 0x05, // MQTT version 5
            0x0a, // Will QoS 1 without Will Flag, plus Clean Start
            0x00, 0x3c, // Keep Alive: 60
            0x00, // No properties
            0x00, 0x03, b'a', b'b', b'c', // Client ID: "abc"
        ];

        assert_eq!(decode_connect(&frame), Err(DecodeError::Malformed));
    }

    #[test]
    fn will_qos_three_is_malformed() {
        let frame = [
            0x10, 0x10, // CONNECT, Remaining Length 16
            0x00, 0x04, b'M', b'Q', b'T', b'T', 0x05, // MQTT version 5
            0x1e, // Will Flag, invalid Will QoS 3, and Clean Start
            0x00, 0x3c, // Keep Alive: 60
            0x00, // No properties
            0x00, 0x03, b'a', b'b', b'c', // Client ID: "abc"
        ];

        assert_eq!(decode_connect(&frame), Err(DecodeError::Malformed));
    }

    #[test]
    fn request_problem_information_is_decoded() {
        let frame = [
            0x10, 0x12, // CONNECT, Remaining Length 18
            0x00, 0x04, b'M', b'Q', b'T', b'T', 0x05, // MQTT version 5
            0x02, // Clean Start
            0x00, 0x3c, // Keep Alive: 60
            0x02, // Property Length: 2
            0x17, 0x00, // Request Problem Information: false
            0x00, 0x03, b'a', b'b', b'c', // Client ID: "abc"
        ];

        assert_eq!(
            decode_connect(&frame),
            Ok(Connect {
                client_id: "abc".to_owned(),
                clean_start: true,
                keep_alive: 60,
                request_problem_information: false,
                maximum_packet_size: u32::MAX,
            })
        );
    }

    #[test]
    fn unterminated_property_identifier_is_malformed() {
        let frame = [
            0x10, 0x11, // CONNECT, Remaining Length 17
            0x00, 0x04, b'M', b'Q', b'T', b'T', 0x05, // MQTT version 5
            0x02, // Clean Start
            0x00, 0x3c, // Keep Alive: 60
            0x01, // Property Length: 1
            0x80, // Unterminated Variable Byte Integer property identifier
            0x00, 0x03, b'a', b'b', b'c', // Client ID: "abc"
        ];

        assert_eq!(decode_connect(&frame), Err(DecodeError::Malformed));
    }

    #[test]
    fn duplicate_request_problem_information_is_malformed() {
        let frame = [
            0x10, 0x14, // CONNECT, Remaining Length 20
            0x00, 0x04, b'M', b'Q', b'T', b'T', 0x05, // MQTT version 5
            0x02, // Clean Start
            0x00, 0x3c, // Keep Alive: 60
            0x04, // Property Length: 4
            0x17, 0x00, // Request Problem Information: false
            0x17, 0x01, // Duplicate Request Problem Information: true
            0x00, 0x03, b'a', b'b', b'c', // Client ID: "abc"
        ];

        assert_eq!(decode_connect(&frame), Err(DecodeError::Malformed));
    }

    #[test]
    fn truncated_connect_is_incomplete() {
        let frame = [
            0x10, 0x10, // CONNECT, Remaining Length 16
            0x00, 0x04, b'M', b'Q', b'T', b'T', 0x05, // MQTT version 5
            0x02, // Clean Start
            0x00, 0x3c, // Keep Alive: 60
            0x00, // No properties
            0x00, 0x03, b'a', b'b', b'c', // Client ID: "abc"
        ];

        for end in 0..frame.len() {
            assert_eq!(decode_connect(&frame[..end]), Err(DecodeError::Incomplete));
        }
    }

    #[test]
    fn successful_connack_advertises_phase_one_capabilities() {
        assert_eq!(
            encode_connack(),
            vec![
                0x20, 0x0d, // CONNACK, Remaining Length 13
                0x00, // Session Present: false
                0x00, // Success
                0x0a, // Property Length: 10
                0x24, 0x00, // Maximum QoS: 0
                0x25, 0x00, // Retain Available: false
                0x28, 0x01, // Wildcard Subscription Available: true
                0x29, 0x00, // Subscription Identifier Available: false
                0x2a, 0x00, // Shared Subscription Available: false
            ]
        );
    }

    #[test]
    fn topic_alias_is_malformed_in_connect() {
        let frame = [
            0x10, 0x13, // CONNECT, Remaining Length 19
            0x00, 0x04, b'M', b'Q', b'T', b'T', 0x05, // MQTT version 5
            0x02, // Clean Start
            0x00, 0x3c, // Keep Alive: 60
            0x03, // Property Length: 3
            0x23, 0x00, 0x01, // Topic Alias: 1 (not allowed in CONNECT)
            0x00, 0x03, b'a', b'b', b'c', // Client ID: "abc"
        ];

        assert_eq!(decode_connect(&frame), Err(DecodeError::Malformed));
    }

    #[test]
    fn client_id_exceeding_frame_is_malformed() {
        let frame = [
            0x10, 0x10, // CONNECT, Remaining Length 16
            0x00, 0x04, b'M', b'Q', b'T', b'T', 0x05, // MQTT version 5
            0x02, // Clean Start
            0x00, 0x3c, // Keep Alive: 60
            0x00, // No properties
            0x00, 0x04, b'a', b'b', b'c', // Client ID claims 4 bytes, only 3 follow
        ];

        assert_eq!(decode_connect(&frame), Err(DecodeError::Malformed));
    }

    #[test]
    fn four_byte_integer_is_decoded() {
        assert_eq!(decode_u32(&[0x00, 0x00, 0x04, 0x00]), Ok((1024, 4))); // Big-endian value 1024
    }

    #[test]
    fn incomplete_four_byte_integer_is_reported() {
        assert_eq!(decode_u32(&[]), Err(DecodeError::Incomplete)); // No bytes
        assert_eq!(
            decode_u32(&[0x00, 0x00, 0x04]), // Only 3 of 4 bytes
            Err(DecodeError::Incomplete)
        );
    }

    #[test]
    fn maximum_packet_size_is_decoded() {
        let frame = [
            0x10, 0x15, // CONNECT, Remaining Length 21
            0x00, 0x04, b'M', b'Q', b'T', b'T', 0x05, // MQTT version 5
            0x02, // Clean Start
            0x00, 0x3c, // Keep Alive: 60
            0x05, // Property Length: 5
            0x27, 0x00, 0x00, 0x04, 0x00, // Maximum Packet Size: 1024
            0x00, 0x03, b'a', b'b', b'c', // Client ID: "abc"
        ];

        assert_eq!(
            decode_connect(&frame),
            Ok(Connect {
                client_id: "abc".to_owned(),
                clean_start: true,
                keep_alive: 60,
                request_problem_information: true,
                maximum_packet_size: 1024,
            })
        );
    }

    #[test]
    fn zero_maximum_packet_size_is_malformed() {
        let frame = [
            0x10, 0x15, // CONNECT, Remaining Length 21
            0x00, 0x04, b'M', b'Q', b'T', b'T', 0x05, // MQTT version 5
            0x02, // Clean Start
            0x00, 0x3c, // Keep Alive: 60
            0x05, // Property Length: 5
            0x27, 0x00, 0x00, 0x00, 0x00, // Maximum Packet Size: 0 (invalid)
            0x00, 0x03, b'a', b'b', b'c', // Client ID: "abc"
        ];

        assert_eq!(decode_connect(&frame), Err(DecodeError::Malformed));
    }

    #[test]
    fn duplicate_maximum_packet_size_is_malformed() {
        let frame = [
            0x10, 0x1a, // CONNECT, Remaining Length 26
            0x00, 0x04, b'M', b'Q', b'T', b'T', 0x05, // MQTT version 5
            0x02, // Clean Start
            0x00, 0x3c, // Keep Alive: 60
            0x0a, // Property Length: 10
            0x27, 0x00, 0x00, 0x00, 0x01, // Maximum Packet Size: 1
            0x27, 0x00, 0x00, 0x00, 0x01, // Duplicate Maximum Packet Size: 1
            0x00, 0x03, b'a', b'b', b'c', // Client ID: "abc"
        ];

        assert_eq!(decode_connect(&frame), Err(DecodeError::Malformed));
    }

    #[test]
    fn invalid_request_problem_information_is_malformed() {
        let frame = [
            0x10, 0x12, // CONNECT, Remaining Length 18
            0x00, 0x04, b'M', b'Q', b'T', b'T', 0x05, // MQTT version 5
            0x02, // Clean Start
            0x00, 0x3c, // Keep Alive: 60
            0x02, // Property Length: 2
            0x17, 0x02, // Request Problem Information: 2 (invalid)
            0x00, 0x03, b'a', b'b', b'c', // Client ID: "abc"
        ];

        assert_eq!(decode_connect(&frame), Err(DecodeError::Malformed));
    }

    #[test]
    fn truncated_maximum_packet_size_is_malformed() {
        let frame = [
            0x10, 0x14, // CONNECT, Remaining Length 20
            0x00, 0x04, b'M', b'Q', b'T', b'T', 0x05, // MQTT version 5
            0x02, // Clean Start
            0x00, 0x3c, // Keep Alive: 60
            0x04, // Property Length: 4
            0x27, 0x00, 0x00, 0x01, // Truncated Maximum Packet Size value
            0x00, 0x03, b'a', b'b', b'c', // Client ID: "abc"
        ];

        assert_eq!(decode_connect(&frame), Err(DecodeError::Malformed));
    }

    #[test]
    fn multiple_connect_properties_are_decoded() {
        let frame = [
            0x10, 0x17, // CONNECT, Remaining Length 23
            0x00, 0x04, b'M', b'Q', b'T', b'T', 0x05, // MQTT version 5
            0x02, // Clean Start
            0x00, 0x3c, // Keep Alive: 60
            0x07, // Property Length: 7
            0x17, 0x00, // Request Problem Information: false
            0x27, 0x00, 0x00, 0x04, 0x00, // Maximum Packet Size: 1024
            0x00, 0x03, b'a', b'b', b'c', // Client ID: "abc"
        ];

        assert_eq!(
            decode_connect(&frame),
            Ok(Connect {
                client_id: "abc".to_owned(),
                clean_start: true,
                keep_alive: 60,
                request_problem_information: false,
                maximum_packet_size: 1024,
            })
        );
    }

    #[test]
    fn pingreq_packet_type_is_decoded() {
        assert_eq!(decode_packet_type(0xc0), Ok(PacketType::PingReq)); // PINGREQ, flags 0
    }

    #[test]
    fn pingreq_with_non_zero_flags_is_malformed() {
        assert_eq!(decode_packet_type(0xc1), Err(DecodeError::Malformed)); // PINGREQ, reserved flag set
    }

    #[test]
    fn pingreq_is_decoded() {
        assert_eq!(decode_pingreq(&[0xc0, 0x00]), Ok(())); // PINGREQ, empty body
    }

    #[test]
    fn truncated_pingreq_is_incomplete() {
        assert_eq!(decode_pingreq(&[0xc0]), Err(DecodeError::Incomplete)); // PINGREQ without Remaining Length
    }

    #[test]
    fn pingreq_with_non_zero_remaining_length_is_malformed() {
        assert_eq!(
            decode_pingreq(&[
                0xc0, 0x01, // PINGREQ, Remaining Length 1 (must be 0)
                0x00, // Unexpected body byte
            ]),
            Err(DecodeError::Malformed)
        );
    }

    #[test]
    fn pingresp_is_encoded() {
        assert_eq!(encode_pingresp(), vec![0xd0, 0x00]); // PINGRESP, empty body
    }

    #[test]
    fn pingreq_packet_is_decoded() {
        assert_eq!(decode_packet(&[0xc0, 0x00]), Ok(Packet::PingReq)); // PINGREQ, empty body
    }

    #[test]
    fn malformed_pingreq_packet_is_rejected() {
        assert_eq!(
            decode_packet(&[
                0xc0, 0x01, // PINGREQ, Remaining Length 1 (must be 0)
                0x00, // Unexpected body byte
            ]),
            Err(DecodeError::Malformed)
        );
    }

    #[test]
    fn connect_packet_is_decoded() {
        let frame = [
            0x10, 0x10, // CONNECT, Remaining Length 16
            0x00, 0x04, b'M', b'Q', b'T', b'T', 0x05, // MQTT version 5
            0x02, // Clean Start
            0x00, 0x3c, // Keep Alive: 60
            0x00, // No properties
            0x00, 0x03, b'a', b'b', b'c', // Client ID: "abc"
        ];

        assert_eq!(
            decode_packet(&frame),
            Ok(Packet::Connect(Connect {
                client_id: "abc".to_owned(),
                clean_start: true,
                keep_alive: 60,
                request_problem_information: true,
                maximum_packet_size: u32::MAX,
            }))
        );
    }

    #[test]
    fn disconnect_packet_is_decoded() {
        assert_eq!(decode_packet(&[0xe0, 0x00]), Ok(Packet::Disconnect));
    }

    #[test]
    fn disconnect_packet_with_trailing_bytes_is_malformed() {
        assert_eq!(
            decode_packet(&[0xe0, 0x00, 0x00]),
            Err(DecodeError::Malformed)
        );
    }

    #[test]
    fn disconnect_decoder_rejects_another_packet_type() {
        assert_eq!(
            decode_disconnect(&[0xc0, 0x00]),
            Err(DecodeError::Malformed)
        );
    }
}
