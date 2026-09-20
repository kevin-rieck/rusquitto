use mqtt_codec::Packet;

#[derive(Debug, PartialEq)]
pub enum ConnectionState {
    AwaitingConnect,
    Connected,
}

#[derive(Debug, PartialEq)]
pub enum ConnectionAction {
    SendConnAck,
}

#[derive(Debug, PartialEq)]
pub enum ProtocolError {
    UnexpectedPacket,
}

pub fn handle_packet(
    state: &mut ConnectionState,
    packet: Packet,
) -> Result<ConnectionAction, ProtocolError> {
    if *state != ConnectionState::AwaitingConnect {
        return Err(ProtocolError::UnexpectedPacket);
    }

    match packet {
        Packet::Connect(_) => {
            *state = ConnectionState::Connected;
            Ok(ConnectionAction::SendConnAck)
        }
        _ => Err(ProtocolError::UnexpectedPacket),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mqtt_codec::{Connect, Packet};

    #[test]
    fn connect_transitions_to_connected() {
        let mut state = ConnectionState::AwaitingConnect;
        let packet = Packet::Connect(Connect {
            client_id: "abc".to_owned(),
            clean_start: true,
            keep_alive: 60,
            request_problem_information: true,
            maximum_packet_size: u32::MAX,
        });

        assert_eq!(
            handle_packet(&mut state, packet),
            Ok(ConnectionAction::SendConnAck)
        );
        assert_eq!(state, ConnectionState::Connected);
    }
}
