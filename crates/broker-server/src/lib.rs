use mqtt_codec::{DecodeError, Packet, decode_packet, encode_connack, encode_pingresp};

#[derive(Debug, PartialEq)]
pub enum ConnectionState {
    AwaitingConnect,
    Connected,
}

#[derive(Debug, PartialEq)]
pub enum ConnectionAction {
    SendConnAck,
    SendPingResp,
}

#[derive(Debug, PartialEq)]
pub enum ProtocolError {
    ConnectRequired,
    DuplicateConnect,
}

#[derive(Debug, PartialEq)]
pub enum AdapterError {
    Decode(DecodeError),
    Protocol(ProtocolError),
}

pub fn handle_packet(
    state: &mut ConnectionState,
    packet: Packet,
) -> Result<ConnectionAction, ProtocolError> {
    match *state {
        ConnectionState::AwaitingConnect => match packet {
            Packet::Connect(_) => {
                *state = ConnectionState::Connected;
                Ok(ConnectionAction::SendConnAck)
            }
            _ => Err(ProtocolError::ConnectRequired),
        },

        ConnectionState::Connected => match packet {
            Packet::PingReq => Ok(ConnectionAction::SendPingResp),
            Packet::Connect(_) => Err(ProtocolError::DuplicateConnect),
        },
    }
}

pub fn process_frame(state: &mut ConnectionState, input: &[u8]) -> Result<Vec<u8>, AdapterError> {
    let packet = decode_packet(input).map_err(AdapterError::Decode)?;

    let connection_action = handle_packet(state, packet).map_err(AdapterError::Protocol)?;
    match connection_action {
        ConnectionAction::SendConnAck => Ok(encode_connack()),
        ConnectionAction::SendPingResp => Ok(encode_pingresp()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mqtt_codec::{Connect, Packet};

    fn connect_packet() -> Packet {
        Packet::Connect(Connect {
            client_id: "abc".to_owned(),
            clean_start: true,
            keep_alive: 60,
            request_problem_information: true,
            maximum_packet_size: u32::MAX,
        })
    }

    #[test]
    fn connect_transitions_to_connected() {
        let mut state = ConnectionState::AwaitingConnect;
        let packet = connect_packet();

        assert_eq!(
            handle_packet(&mut state, packet),
            Ok(ConnectionAction::SendConnAck)
        );
        assert_eq!(state, ConnectionState::Connected);
    }

    #[test]
    fn pingreq_before_connect_is_rejected() {
        let mut state = ConnectionState::AwaitingConnect;
        let packet = Packet::PingReq;
        assert_eq!(
            handle_packet(&mut state, packet),
            Err(ProtocolError::ConnectRequired)
        );
        assert_eq!(state, ConnectionState::AwaitingConnect);
    }

    #[test]
    fn pingreq_when_connected_sends_pingresp() {
        let mut state = ConnectionState::Connected;
        let packet = Packet::PingReq;
        assert_eq!(
            handle_packet(&mut state, packet),
            Ok(ConnectionAction::SendPingResp)
        );
        assert_eq!(state, ConnectionState::Connected);
    }

    #[test]
    fn duplicate_connect_is_rejected() {
        let mut state = ConnectionState::Connected;
        let packet = connect_packet();
        assert_eq!(
            handle_packet(&mut state, packet),
            Err(ProtocolError::DuplicateConnect)
        );
        assert_eq!(state, ConnectionState::Connected);
    }

    #[test]
    fn connect_frame_produces_connack_and_transitions_to_connected() {
        let mut state = ConnectionState::AwaitingConnect;
        let frame = [
            0x10, 0x10, // CONNECT, Remaining Length 16
            0x00, 0x04, b'M', b'Q', b'T', b'T', 0x05, 0x02, // Clean Start
            0x00, 0x3c, // Keep Alive: 60
            0x00, // No properties
            0x00, 0x03, b'a', b'b', b'c',
        ];

        let response = process_frame(&mut state, &frame).unwrap();

        assert_eq!(
            response,
            vec![
                0x20, 0x0d, 0x00, 0x00, 0x0a, 0x24, 0x00, 0x25, 0x00, 0x28, 0x01, 0x29, 0x00, 0x2a,
                0x00,
            ]
        );
        assert_eq!(state, ConnectionState::Connected);
    }

    #[test]
    fn connected_pingreq_frame_produces_pingresp() {
        let mut state = ConnectionState::Connected;
        let frame = [0xc0, 0x00];

        let response = process_frame(&mut state, &frame).unwrap();
        assert_eq!(state, ConnectionState::Connected);
        assert_eq!(response, vec![0xd0, 0x00]);
    }
}
