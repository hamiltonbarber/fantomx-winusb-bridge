/// USB-MIDI 1.0 Codec (Packetizer / Depacketizer)
/// Conforms to USB Device Class Definition for MIDI Devices v1.0

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MidiMessage {
    NoteOff { channel: u8, note: u8, velocity: u8 },
    NoteOn { channel: u8, note: u8, velocity: u8 },
    PolyAftertouch { channel: u8, note: u8, pressure: u8 },
    ControlChange { channel: u8, controller: u8, value: u8 },
    ProgramChange { channel: u8, program: u8 },
    ChannelAftertouch { channel: u8, pressure: u8 },
    PitchBend { channel: u8, value: u16 },
    SysEx(Vec<u8>),
    Clock,
    Start,
    Continue,
    Stop,
    ActiveSensing,
    Reset,
    Raw(Vec<u8>),
}

pub struct UsbMidiDecoder {
    sysex_accumulator: Vec<u8>,
}

impl UsbMidiDecoder {
    pub fn new() -> Self {
        Self {
            sysex_accumulator: Vec::with_capacity(256),
        }
    }

    pub fn decode_packet(&mut self, pkt: &[u8]) -> Option<MidiMessage> {
        if pkt.len() < 4 {
            return None;
        }

        let cin = pkt[0] & 0x0F;
        let _cable = (pkt[0] >> 4) & 0x0F;
        let b0 = pkt[1];
        let b1 = pkt[2];
        let b2 = pkt[3];

        match cin {
            0x8 => {
                let channel = (b0 & 0x0F) + 1;
                Some(MidiMessage::NoteOff { channel, note: b1, velocity: b2 })
            }
            0x9 => {
                let channel = (b0 & 0x0F) + 1;
                if b2 == 0 {
                    Some(MidiMessage::NoteOff { channel, note: b1, velocity: 0 })
                } else {
                    Some(MidiMessage::NoteOn { channel, note: b1, velocity: b2 })
                }
            }
            0xA => {
                let channel = (b0 & 0x0F) + 1;
                Some(MidiMessage::PolyAftertouch { channel, note: b1, pressure: b2 })
            }
            0xB => {
                let channel = (b0 & 0x0F) + 1;
                Some(MidiMessage::ControlChange { channel, controller: b1, value: b2 })
            }
            0xC => {
                let channel = (b0 & 0x0F) + 1;
                Some(MidiMessage::ProgramChange { channel, program: b1 })
            }
            0xD => {
                let channel = (b0 & 0x0F) + 1;
                Some(MidiMessage::ChannelAftertouch { channel, pressure: b1 })
            }
            0xE => {
                let channel = (b0 & 0x0F) + 1;
                let value = ((b2 as u16) << 7) | (b1 as u16);
                Some(MidiMessage::PitchBend { channel, value })
            }
            0x4 => {
                // SysEx starts or continues (3 bytes)
                self.sysex_accumulator.push(b0);
                self.sysex_accumulator.push(b1);
                self.sysex_accumulator.push(b2);
                None
            }
            0x5 => {
                // SysEx ends with 1 byte (b0)
                self.sysex_accumulator.push(b0);
                let full_sysex = std::mem::take(&mut self.sysex_accumulator);
                Some(MidiMessage::SysEx(full_sysex))
            }
            0x6 => {
                // SysEx ends with 2 bytes (b0, b1)
                self.sysex_accumulator.push(b0);
                self.sysex_accumulator.push(b1);
                let full_sysex = std::mem::take(&mut self.sysex_accumulator);
                Some(MidiMessage::SysEx(full_sysex))
            }
            0x7 => {
                // SysEx ends with 3 bytes (b0, b1, b2)
                self.sysex_accumulator.push(b0);
                self.sysex_accumulator.push(b1);
                self.sysex_accumulator.push(b2);
                let full_sysex = std::mem::take(&mut self.sysex_accumulator);
                Some(MidiMessage::SysEx(full_sysex))
            }
            0xF => {
                // Single byte real-time message
                match b0 {
                    0xF8 => Some(MidiMessage::Clock),
                    0xFA => Some(MidiMessage::Start),
                    0xFB => Some(MidiMessage::Continue),
                    0xFC => Some(MidiMessage::Stop),
                    0xFE => Some(MidiMessage::ActiveSensing),
                    0xFF => Some(MidiMessage::Reset),
                    _ => Some(MidiMessage::Raw(vec![b0])),
                }
            }
            _ => None,
        }
    }
}

pub struct UsbMidiEncoder;

impl UsbMidiEncoder {
    pub fn encode_message(msg: &MidiMessage, cable: u8) -> Vec<[u8; 4]> {
        let cable_nibble = (cable & 0x0F) << 4;
        match msg {
            MidiMessage::NoteOff { channel, note, velocity } => {
                let status = 0x80 | ((channel - 1) & 0x0F);
                vec![[cable_nibble | 0x08, status, *note, *velocity]]
            }
            MidiMessage::NoteOn { channel, note, velocity } => {
                let status = 0x90 | ((channel - 1) & 0x0F);
                vec![[cable_nibble | 0x09, status, *note, *velocity]]
            }
            MidiMessage::PolyAftertouch { channel, note, pressure } => {
                let status = 0xA0 | ((channel - 1) & 0x0F);
                vec![[cable_nibble | 0x0A, status, *note, *pressure]]
            }
            MidiMessage::ControlChange { channel, controller, value } => {
                let status = 0xB0 | ((channel - 1) & 0x0F);
                vec![[cable_nibble | 0x0B, status, *controller, *value]]
            }
            MidiMessage::ProgramChange { channel, program } => {
                let status = 0xC0 | ((channel - 1) & 0x0F);
                vec![[cable_nibble | 0x0C, status, *program, 0x00]]
            }
            MidiMessage::ChannelAftertouch { channel, pressure } => {
                let status = 0xD0 | ((channel - 1) & 0x0F);
                vec![[cable_nibble | 0x0D, status, *pressure, 0x00]]
            }
            MidiMessage::PitchBend { channel, value } => {
                let status = 0xE0 | ((channel - 1) & 0x0F);
                let lsb = (value & 0x7F) as u8;
                let msb = ((value >> 7) & 0x7F) as u8;
                vec![[cable_nibble | 0x0E, status, lsb, msb]]
            }
            MidiMessage::Clock => vec![[cable_nibble | 0x0F, 0xF8, 0x00, 0x00]],
            MidiMessage::Start => vec![[cable_nibble | 0x0F, 0xFA, 0x00, 0x00]],
            MidiMessage::Continue => vec![[cable_nibble | 0x0F, 0xFB, 0x00, 0x00]],
            MidiMessage::Stop => vec![[cable_nibble | 0x0F, 0xFC, 0x00, 0x00]],
            MidiMessage::ActiveSensing => vec![[cable_nibble | 0x0F, 0xFE, 0x00, 0x00]],
            MidiMessage::Reset => vec![[cable_nibble | 0x0F, 0xFF, 0x00, 0x00]],
            MidiMessage::SysEx(data) => {
                let mut packets = Vec::new();
                let mut idx = 0;
                let len = data.len();
                while idx < len {
                    let remaining = len - idx;
                    if remaining >= 3 {
                        if remaining == 3 {
                            packets.push([cable_nibble | 0x07, data[idx], data[idx + 1], data[idx + 2]]);
                        } else {
                            packets.push([cable_nibble | 0x04, data[idx], data[idx + 1], data[idx + 2]]);
                        }
                        idx += 3;
                    } else if remaining == 2 {
                        packets.push([cable_nibble | 0x06, data[idx], data[idx + 1], 0x00]);
                        idx += 2;
                    } else if remaining == 1 {
                        packets.push([cable_nibble | 0x05, data[idx], 0x00, 0x00]);
                        idx += 1;
                    }
                }
                packets
            }
            MidiMessage::Raw(raw) => {
                if raw.len() == 1 {
                    vec![[cable_nibble | 0x0F, raw[0], 0x00, 0x00]]
                } else if raw.len() == 2 {
                    vec![[cable_nibble | 0x02, raw[0], raw[1], 0x00]]
                } else if raw.len() >= 3 {
                    vec![[cable_nibble | 0x03, raw[0], raw[1], raw[2]]]
                } else {
                    Vec::new()
                }
            }
        }
    }
}
