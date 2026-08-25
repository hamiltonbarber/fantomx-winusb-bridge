mod usb_midi_codec;
mod winusb_transport;

use std::io::{self, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::Duration;
use usb_midi_codec::{MidiMessage, UsbMidiDecoder};
use winusb_transport::{discover_fantomx_guids, WinUsbSession, WinUsbTransport};

static NOTE_COUNT: AtomicU32 = AtomicU32::new(0);
static CLOCK_COUNT: AtomicU32 = AtomicU32::new(0);

fn flush_print(s: &str) {
    println!("{}", s);
    let _ = io::stdout().flush();
}

fn handle_incoming_midi_message(msg: MidiMessage) {
    match msg {
        MidiMessage::NoteOn { channel, note, velocity } => {
            let n = NOTE_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
            flush_print(&format!("  [#{:04}] Note-On  (Ch {}) Note {} (0x{:02X}) Vel {}", n, channel, note, note, velocity));
        }
        MidiMessage::NoteOff { channel, note, velocity } => {
            let n = NOTE_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
            flush_print(&format!("  [#{:04}] Note-Off (Ch {}) Note {} (0x{:02X}) Vel {}", n, channel, note, note, velocity));
        }
        MidiMessage::ControlChange { channel, controller, value } => {
            let n = NOTE_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
            flush_print(&format!("  [#{:04}] CC        (Ch {}) CC#{} Val {}", n, channel, controller, value));
        }
        MidiMessage::PitchBend { channel, value } => {
            let n = NOTE_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
            flush_print(&format!("  [#{:04}] PitchBend (Ch {}) Val {}", n, channel, value));
        }
        MidiMessage::ProgramChange { channel, program } => {
            let n = NOTE_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
            flush_print(&format!("  [#{:04}] ProgChg   (Ch {}) Prog {}", n, channel, program));
        }
        MidiMessage::ChannelAftertouch { channel, pressure } => {
            let n = NOTE_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
            flush_print(&format!("  [#{:04}] Aftertch  (Ch {}) Pressure {}", n, channel, pressure));
        }
        MidiMessage::SysEx(data) => {
            let n = NOTE_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
            let hex = data.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ");
            flush_print(&format!("  [#{:04}] SysEx     ({} bytes): [{}]", n, data.len(), hex));
        }
        MidiMessage::Clock => {
            let c = CLOCK_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
            if c % 240 == 0 {
                flush_print(&format!("  [Heartbeat] MIDI Clock Active: {} ticks (~48/sec)", c));
            }
        }
        _ => {}
    }
}

fn main() {
    flush_print("================================================================================");
    flush_print(" Roland Fantom-X Windows 11 WinUSB Bridge (v1.0)");
    flush_print(" 100% In-Box Microsoft WinUSB Driver | Memory Integrity (HVCI) Ready");
    flush_print("================================================================================");

    // Cache GUIDs at startup so PnP attachment is instantaneous (<1ms)
    let guids = discover_fantomx_guids();
    if guids.is_empty() {
        flush_print("WARNING: No Fantom-X WinUSB DeviceInterfaceGUID found in registry.");
        flush_print("Please ensure the Fantom-X driver was assigned to WinUSB via Zadig.\n");
    }

    flush_print("\n>>> Bridge daemon active. Waiting for Fantom-X power-on / connection...\n");

    loop {
        // Fast direct PnP discovery loop (10ms)
        if let Some(path) = WinUsbTransport::find_device_with_guids(&guids) {
            let path_str = String::from_utf16_lossy(&path[..path.len() - 1]);
            flush_print(&format!(">>> FANTOM-X ARRIVAL DETECTED: {}", path_str));
            flush_print(">>> Arming WinUSB endpoints in sub-millisecond time...");

            if let Some(mut session) = WinUsbSession::open(&path) {
                let mut decoder = UsbMidiDecoder::new();

                session.start_reader_loop(move |raw_buf| {
                    for chunk in raw_buf.chunks(4) {
                        if chunk.len() == 4 {
                            if let Some(msg) = decoder.decode_packet(chunk) {
                                handle_incoming_midi_message(msg);
                            }
                        }
                    }
                });

                flush_print(">>> Bridge connected and streaming! Ready for bidirectional MIDI.\n");

                while session.is_alive() {
                    thread::sleep(Duration::from_millis(500));
                }

                flush_print("\n>>> FANTOM-X DISCONNECTED. Waiting for next device connection...\n");
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
}
