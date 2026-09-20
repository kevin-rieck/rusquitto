use mqtt_codec::Packet;

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
    UnexpectedPacket,
    ConnectRequired,
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
            _ => Err(ProtocolError::UnexpectedPacket),
        },
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
}
