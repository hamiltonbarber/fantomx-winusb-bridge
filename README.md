# fantomx-winusb-bridge

A lightweight, high-performance **Windows 11** user-space WinUSB MIDI bridge for the Roland Fantom-X series synthesizers. Replaces Roland's discontinued kernel driver (`RDWM1045.SYS`) with Microsoft's in-box `WinUSB.sys` and Microsoft's official **Windows MIDI Services**, verified working on Windows 11 with Memory Integrity / HVCI enabled and without Roland's legacy driver.

> **Hardware Compatibility**: Tested on Fantom-X8; expected to support other Roland Fantom-X family instruments sharing USB Hardware ID `USB\VID_0582&PID_006D` (X6, X7, XR, Xa).

## Features

### Seamless DAW Integration
- **Direct DAW connectivity** - Exposes a native Windows MIDI port (`Roland Fantom-X`) to FL Studio, Ableton Live, Reaper, Cubase, Studio One, and other DAWs
- **Bidirectional streaming** - Low-latency capture of Note-On/Off (with velocity), 14-bit Pitch Bend, Modulation/CC, Polyphonic Aftertouch, Program Changes, and MIDI Real-Time Clocks, plus playback to the internal sound engine
- **Bridge-lifetime DAW endpoint** - The DAW port survives Fantom-X hardware power cycles while the bridge daemon is running, allowing the synthesizer to be turned off and on without invalidating DAW device handles

### Modern Windows Architecture
- **No custom or legacy kernel driver** - Operates in user-space via Microsoft's in-box, WHQL-signed `WinUSB.sys`
- **Dynamic GUID discovery** - Automatically resolves the device interface GUID assigned by Windows/Zadig without hard-coding
- **Low-latency attach polling** - 10 ms device-detection polling interval, verified to arm the Fantom-X successfully on the tested system
- **Verified with Memory Integrity (HVCI)** - Tested and verified operating with Windows 11 Hypervisor-Protected Code Integrity enabled

## Architecture

```text
[ Roland Fantom-X Hardware ]
             ↕ (Microsoft WinUSB.sys: OUT 0x01, IN 0x82)
[ fantomx-bridge Daemon ]
             ↕ (32-bit Universal MIDI Packet Stream)
[ Windows MIDI Services (A/B Loopback Transport) ]
             ↕ (WinMM / UMP)
[ FL Studio / Ableton / Reaper / DAWs ]
```

## Requirements

- **Operating System**: Windows 11 (64-bit, 24H2 / 25H2 or newer)
- **Hardware**: Roland Fantom-X synthesizer (USB Mode set to MIDI)
- **Driver**: Microsoft WinUSB (assigned via Zadig)
- **Windows MIDI Services**: Microsoft Windows MIDI Services Runtime & Loopback Transport ([Microsoft GitHub](https://github.com/microsoft/MIDI/releases))

## Quick Start

### 1. Synthesizer Configuration
1. Power on your Fantom-X.
2. Press **MENU** -> **System** -> **USB**.
3. Set **USB Mode** to **`MIDI`**.
4. Press **Write** to save settings.

### 2. Driver Setup (WinUSB)
1. Connect the Fantom-X to your PC via USB.
2. Download [Zadig](https://zadig.akeo.ie/) ([GitHub: pbatard/libwdi](https://github.com/pbatard/libwdi)).
3. In Zadig:
   - Go to **Options** -> check **List All Devices**.
   - Select **FANTOM-X** from the dropdown (`VID: 0582`, `PID: 006D`).
   - Select **WinUSB** as the target driver.
   - Click **Replace Driver**.

### 3. Running the Bridge
Double-click `run_bridge.bat`. The console will display:
```text
>>> Endpoint A (Host Bridge): Fantom-X Bridge Host
>>> Endpoint B (DAW Client):  Roland Fantom-X
>>> [WinMM Ready] Input Port: "Roland Fantom-X" -> OK
>>> SUCCESS: "Roland Fantom-X" is fully published and ready for your DAW!
>>> FANTOM-X ATTACHED & STREAMING TO DAW!
```

### 4. DAW Configuration
1. Open your DAW (e.g. FL Studio -> **Options** -> **MIDI Settings**).
2. Look for **`Roland Fantom-X`** under **Input** and **Output**.
3. **Input**: Click **Enable** (the indicator/power icon will turn green).
4. **Output**: Set the output **Port** to `1` (for routing MIDI Out / Piano Roll notes to the synthesizer sound engine).
5. Play keys on your Fantom-X to record and play instruments in real time!

---

## Building from Source

To compile the C++/WinRT bridge from source:

1. Install **Visual Studio 2022** (with Desktop development with C++).
2. Run `build.bat` in the repository root:
   ```cmd
   build.bat
   ```
   This will restore required NuGet packages (`Microsoft.Windows.CppWinRT`, `Windows.Devices.Midi2`) and compile `bin\x64\Release\fantomx-bridge.exe`.

---

## Troubleshooting

### 1. Clearing Duplicate Ghost Ports (`Roland Fantom-X #2`, `#3`, `#4`...)
> **IMPORTANT**: If multiple bridge test sessions were started or interrupted, Windows MIDI Services (`MidiSrv`) maintains previous endpoint registrations in background memory. This causes DAWs to display numbered copies (`#2`, `#3`, `#4`), and connecting to an inactive copy will result in silence.

**How to reset all ports to a clean single instance**:
1. Close your DAW and close the bridge window.
2. Right-click **`scripts\restart_midi_service.bat`** and select **Run as administrator** (or restart your computer). This immediately flushes all stale endpoint registrations from Windows memory.
3. Launch `run_bridge.bat` and reopen your DAW. You will now see only **one single, active `Roland Fantom-X` port**.

### 2. No MIDI events arrive when playing keys
- **Cause**: The Fantom-X USB microcontroller requires the host application to begin polling within the initial power-up / USB connection window. If the synthesizer was powered on before the bridge daemon was started, it enters an idle state.
- **Solution**: With `run_bridge.bat` open, **turn the Fantom-X power OFF, wait 2 seconds, and turn power ON** (or disconnect and reconnect the USB cable). The bridge will immediately catch the attach event and arm the stream.

### 3. Which MIDI port do I select in my DAW?
- Windows MIDI Services creates an A/B associated loopback pair:
  - **`Roland Fantom-X`**: This is the DAW client port. Select and **Enable** this port in your DAW for both Input and Output.
  - **`Fantom-X Bridge Host`**: This is the internal communication pipe used exclusively by the bridge background process. Leave this unassigned in your DAW.

---

## License

[MIT License](LICENSE) - Copyright (c) 2026 Hamilton Barber
