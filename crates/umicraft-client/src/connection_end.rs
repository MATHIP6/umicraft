// SPDX-License-Identifier: MIT OR Apache-2.0

//! Why a connection ended, in the terms of the vanilla disconnect screens.
//!
//! `GuiConnecting` shows a failed attempt under `connect.failed` and gives
//! "Unknown host" for an `UnknownHostException` and the exception text for
//! anything else; `NetHandlerPlayClient.onDisconnect` shows a dropped
//! session under `disconnect.lost`; `NetworkManager` names a read timeout
//! (`disconnect.timeout`) and a closed stream (`disconnect.endOfStream`).
//! The network thread classifies its error once ([`ConnectionEnd::from_error`])
//! and the window turns it into a title key and a reason text with the
//! player's language ([`ConnectionEnd::title_key`], [`ConnectionEnd::reason`]).

use std::fmt;
use std::io::ErrorKind;

use umicraft_core::lang::Lang;
use umicraft_net::NetError;

/// How far the session had come when it ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndPhase {
    /// Before the server accepted the login: a failed attempt.
    Connecting,
    /// During play: a lost connection.
    Playing,
}

/// The cause of the end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndCause {
    /// The host name did not resolve.
    UnknownHost,
    /// Nothing listens on the port.
    Refused,
    /// The server did not answer in time.
    TimedOut,
    /// The server closed the socket.
    EndOfStream,
    /// The server wants premium authentication, which is not built yet.
    OnlineMode,
    /// The server sent a disconnect with this reason.
    Kicked(String),
    /// Any other failure, with its text.
    Other(String),
}

/// A finished session: the phase it ended in and the cause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionEnd {
    /// Where the session was.
    pub phase: EndPhase,
    /// What went wrong.
    pub cause: EndCause,
}

impl ConnectionEnd {
    /// An end with a cause the network layer does not classify.
    #[must_use]
    pub fn other(phase: EndPhase, text: impl Into<String>) -> Self {
        Self {
            phase,
            cause: EndCause::Other(text.into()),
        }
    }

    /// A kick during play.
    #[must_use]
    pub fn kicked(reason: impl Into<String>) -> Self {
        Self {
            phase: EndPhase::Playing,
            cause: EndCause::Kicked(reason.into()),
        }
    }

    /// Classifies a network error that ended the session in `phase`.
    #[must_use]
    pub fn from_error(phase: EndPhase, error: &NetError) -> Self {
        let cause = match error {
            NetError::UnknownHost { .. } => EndCause::UnknownHost,
            NetError::Io(io) => match io.kind() {
                ErrorKind::ConnectionRefused => EndCause::Refused,
                ErrorKind::TimedOut => EndCause::TimedOut,
                ErrorKind::UnexpectedEof
                | ErrorKind::ConnectionReset
                | ErrorKind::ConnectionAborted
                | ErrorKind::BrokenPipe => EndCause::EndOfStream,
                _ => EndCause::Other(io.to_string()),
            },
            NetError::ConnectionClosed => EndCause::EndOfStream,
            NetError::TimedOut { .. } => EndCause::TimedOut,
            NetError::OnlineModeRequired { .. } => EndCause::OnlineMode,
            NetError::KickedAtLogin { reason } | NetError::KickedAtPlay { reason } => {
                EndCause::Kicked(reason.clone())
            }
            other => EndCause::Other(other.to_string()),
        };
        Self { phase, cause }
    }

    /// The title key of the disconnect screen: `connect.failed` for an
    /// attempt that never reached play, `disconnect.lost` afterwards.
    #[must_use]
    pub fn title_key(&self) -> &'static str {
        match self.phase {
            EndPhase::Connecting => "connect.failed",
            EndPhase::Playing => "disconnect.lost",
        }
    }

    /// The reason line in `lang`.
    ///
    /// A failed attempt wraps the cause in `disconnect.genericReason`, as
    /// `GuiConnecting` does. A session that was lost names the cause
    /// directly (`disconnect.timeout`, `disconnect.endOfStream`) or shows the
    /// server's own reason.
    #[must_use]
    pub fn reason(&self, lang: &Lang) -> String {
        let text = match &self.cause {
            EndCause::UnknownHost => lang.tr("umicraft.connect.unknownHost").to_string(),
            EndCause::Refused => lang.tr("umicraft.connect.refused").to_string(),
            EndCause::TimedOut if self.phase == EndPhase::Playing => {
                lang.tr("disconnect.timeout").to_string()
            }
            EndCause::TimedOut => lang.tr("umicraft.connect.timedOut").to_string(),
            EndCause::EndOfStream => lang.tr("disconnect.endOfStream").to_string(),
            EndCause::OnlineMode => lang.tr("umicraft.connect.onlineMode").to_string(),
            EndCause::Kicked(reason) => return reason.clone(),
            EndCause::Other(text) => text.clone(),
        };
        match self.phase {
            EndPhase::Connecting => lang.tr_args("disconnect.genericReason", &[&text]),
            EndPhase::Playing => text,
        }
    }
}

/// An English diagnostic line for logs, the headless client and the status
/// bar; the screens use [`ConnectionEnd::reason`] instead.
impl fmt::Display for ConnectionEnd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.reason(&Lang::builtin()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn io(kind: ErrorKind) -> NetError {
        NetError::Io(std::io::Error::from(kind))
    }

    fn end(phase: EndPhase, error: &NetError) -> ConnectionEnd {
        ConnectionEnd::from_error(phase, error)
    }

    #[test]
    fn network_errors_map_to_causes() {
        let host = NetError::UnknownHost {
            host: "x.invalid".to_string(),
        };
        let cases = [
            (host, EndCause::UnknownHost),
            (io(ErrorKind::ConnectionRefused), EndCause::Refused),
            (io(ErrorKind::TimedOut), EndCause::TimedOut),
            (io(ErrorKind::UnexpectedEof), EndCause::EndOfStream),
            (NetError::ConnectionClosed, EndCause::EndOfStream),
            (
                NetError::KickedAtLogin {
                    reason: "banned".to_string(),
                },
                EndCause::Kicked("banned".to_string()),
            ),
        ];
        for (error, cause) in cases {
            assert_eq!(end(EndPhase::Connecting, &error).cause, cause, "{error}");
        }
    }

    #[test]
    fn the_phase_picks_the_vanilla_title() {
        let error = io(ErrorKind::ConnectionRefused);
        assert_eq!(
            end(EndPhase::Connecting, &error).title_key(),
            "connect.failed"
        );
        assert_eq!(
            end(EndPhase::Playing, &error).title_key(),
            "disconnect.lost"
        );
    }

    #[test]
    fn a_failed_attempt_gives_the_vanilla_reasons() {
        let lang = Lang::builtin();
        let unknown = NetError::UnknownHost {
            host: "h".to_string(),
        };
        assert_eq!(
            end(EndPhase::Connecting, &unknown).reason(&lang),
            "Unknown host"
        );
        assert_eq!(
            end(EndPhase::Connecting, &io(ErrorKind::ConnectionRefused)).reason(&lang),
            "Connection refused: no further information"
        );
    }

    #[test]
    fn a_lost_session_names_the_cause_or_shows_the_servers_reason() {
        let lang = Lang::builtin();
        assert_eq!(
            end(EndPhase::Playing, &io(ErrorKind::TimedOut)).reason(&lang),
            "Timed out"
        );
        assert_eq!(
            end(EndPhase::Playing, &NetError::ConnectionClosed).reason(&lang),
            "End of stream"
        );
        assert_eq!(ConnectionEnd::kicked("Flying").reason(&lang), "Flying");
    }

    #[test]
    fn an_unclassified_error_keeps_its_text() {
        let end = ConnectionEnd::other(EndPhase::Connecting, "boom");
        assert_eq!(end.reason(&Lang::builtin()), "boom");
    }
}
