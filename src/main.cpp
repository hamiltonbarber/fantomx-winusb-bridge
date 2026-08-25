#include <iostream>
#include <vector>
#include <string>
#include <thread>
#include <atomic>
#include <chrono>
#include <mutex>
#include <Windows.h>
#include <mmeapi.h>
#include <winusb.h>
#include <setupapi.h>
#include <combaseapi.h>

#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Foundation.Collections.h>
#include <winrt/Windows.Devices.Midi2.h>
#include <winrt/Windows.Devices.Midi2.Transports.Loopback.h>
#include <winrt/Windows.Devices.Midi2.Utilities.Messages.h>

#pragma comment(lib, "setupapi.lib")
#pragma comment(lib, "winusb.lib")
#pragma comment(lib, "advapi32.lib")
#pragma comment(lib, "ole32.lib")
#pragma comment(lib, "winmm.lib")

using namespace winrt::Windows::Devices::Midi2;
using namespace winrt::Windows::Devices::Midi2::Transports::Loopback;
using namespace winrt::Windows::Devices::Midi2::Utilities::Messages;

static std::atomic<bool> g_running{ true };
static std::atomic<bool> g_usb_active{ false };
static HANDLE g_file_handle = INVALID_HANDLE_VALUE;
static WINUSB_INTERFACE_HANDLE g_winusb_handle = NULL;
static std::thread g_reader_thread;
static std::mutex g_usb_mutex;
static MidiEndpointConnection g_endpointA{ nullptr };
static winrt::guid g_associationId{};

void CleanupUsbHandles() {
    std::lock_guard<std::mutex> lock(g_usb_mutex);
    g_usb_active = false;

    if (g_file_handle != INVALID_HANDLE_VALUE && g_file_handle != NULL) {
        CancelIoEx(g_file_handle, NULL);
    }

    if (g_reader_thread.joinable()) {
        g_reader_thread.join();
    }

    if (g_winusb_handle != NULL) {
        WinUsb_Free(g_winusb_handle);
        g_winusb_handle = NULL;
    }

    if (g_file_handle != INVALID_HANDLE_VALUE && g_file_handle != NULL) {
        CloseHandle(g_file_handle);
        g_file_handle = INVALID_HANDLE_VALUE;
    }
}

BOOL WINAPI ConsoleCtrlHandler(DWORD) {
    g_running = false;
    if (g_file_handle != INVALID_HANDLE_VALUE && g_file_handle != NULL) {
        CancelIoEx(g_file_handle, NULL);
    }
    return TRUE;
}

std::vector<GUID> DiscoverFantomXGuids() {
    std::vector<GUID> guids;
    HKEY rootKey;
    if (RegOpenKeyExW(HKEY_LOCAL_MACHINE, L"SYSTEM\\CurrentControlSet\\Enum\\USB\\VID_0582&PID_006D", 0, KEY_READ, &rootKey) != ERROR_SUCCESS) {
        return guids;
    }

    DWORD index = 0;
    WCHAR nameBuf[256];
    DWORD nameLen = 256;
    while (RegEnumKeyExW(rootKey, index++, nameBuf, &nameLen, NULL, NULL, NULL, NULL) == ERROR_SUCCESS) {
        nameLen = 256;
        std::wstring paramPath = L"SYSTEM\\CurrentControlSet\\Enum\\USB\\VID_0582&PID_006D\\" + std::wstring(nameBuf) + L"\\Device Parameters";
        HKEY paramKey;
        if (RegOpenKeyExW(HKEY_LOCAL_MACHINE, paramPath.c_str(), 0, KEY_READ, &paramKey) == ERROR_SUCCESS) {
            WCHAR valBuf[1024];
            DWORD valSize = sizeof(valBuf);
            DWORD valType = 0;
            if (RegQueryValueExW(paramKey, L"DeviceInterfaceGUIDs", NULL, &valType, (LPBYTE)valBuf, &valSize) == ERROR_SUCCESS ||
                RegQueryValueExW(paramKey, L"DeviceInterfaceGUID", NULL, &valType, (LPBYTE)valBuf, &valSize) == ERROR_SUCCESS) {
                WCHAR* p = valBuf;
                while (*p) {
                    GUID g;
                    if (IIDFromString(p, &g) == S_OK) {
                        guids.push_back(g);
                    }
                    p += wcslen(p) + 1;
                }
            }
            RegCloseKey(paramKey);
        }
    }
    RegCloseKey(rootKey);
    return guids;
}

std::wstring FindAttachedDevicePath(const std::vector<GUID>& guids) {
    for (const auto& guid : guids) {
        HDEVINFO devInfo = SetupDiGetClassDevsW(&guid, NULL, NULL, DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
        if (devInfo == INVALID_HANDLE_VALUE) continue;

        SP_DEVICE_INTERFACE_DATA ifData = { sizeof(SP_DEVICE_INTERFACE_DATA) };
        if (SetupDiEnumDeviceInterfaces(devInfo, NULL, &guid, 0, &ifData)) {
            DWORD reqSize = 0;
            SetupDiGetDeviceInterfaceDetailW(devInfo, &ifData, NULL, 0, &reqSize, NULL);
            if (reqSize > 0) {
                std::vector<BYTE> buf(reqSize);
                PSP_DEVICE_INTERFACE_DETAIL_DATA_W detail = (PSP_DEVICE_INTERFACE_DETAIL_DATA_W)buf.data();
                detail->cbSize = sizeof(SP_DEVICE_INTERFACE_DETAIL_DATA_W);
                if (SetupDiGetDeviceInterfaceDetailW(devInfo, &ifData, detail, reqSize, NULL, NULL)) {
                    std::wstring path = detail->DevicePath;
                    SetupDiDestroyDeviceInfoList(devInfo);
                    return path;
                }
            }
        }
        SetupDiDestroyDeviceInfoList(devInfo);
    }
    return L"";
}

bool SendRawUsbMidi(const BYTE* data, DWORD len) {
    std::lock_guard<std::mutex> lock(g_usb_mutex);
    if (!g_usb_active || g_winusb_handle == NULL) return false;
    ULONG written = 0;
    OVERLAPPED ov = { 0 };
    ov.hEvent = CreateEventW(NULL, TRUE, FALSE, NULL);
    BOOL ok = WinUsb_WritePipe(g_winusb_handle, 0x01, (PUCHAR)data, len, &written, &ov);
    if (!ok && GetLastError() == ERROR_IO_PENDING) {
        ok = WinUsb_GetOverlappedResult(g_winusb_handle, &ov, &written, TRUE);
    }
    CloseHandle(ov.hEvent);
    return ok == TRUE;
}

void StartUsbReader(WINUSB_INTERFACE_HANDLE winusb) {
    g_reader_thread = std::thread([winusb]() {
        BYTE rxBuf[64];
        while (g_usb_active) {
            OVERLAPPED ov = { 0 };
            ov.hEvent = CreateEventW(NULL, TRUE, FALSE, NULL);
            ULONG transferred = 0;
            BOOL ok = WinUsb_ReadPipe(winusb, 0x82, rxBuf, sizeof(rxBuf), &transferred, &ov);
            if (!ok && GetLastError() == ERROR_IO_PENDING) {
                ok = WinUsb_GetOverlappedResult(winusb, &ov, &transferred, TRUE);
            }
            CloseHandle(ov.hEvent);

            if (!ok || transferred == 0) {
                break;
            }

            for (ULONG i = 0; i + 4 <= transferred; i += 4) {
                BYTE cin = rxBuf[i] & 0x0F;
                BYTE b1 = rxBuf[i + 1];
                BYTE b2 = rxBuf[i + 2];
                BYTE b3 = rxBuf[i + 3];

                if (cin == 0x08 || cin == 0x09 || cin == 0x0A || cin == 0x0B || cin == 0x0E) {
                    uint32_t ump = (0x2 << 28) | (0 << 24) | (b1 << 16) | (b2 << 8) | b3;
                    if (g_endpointA) {
                        g_endpointA.SendSingleMessageWords(MidiClock::Now(), ump);
                    }
                } else if (cin == 0x0C || cin == 0x0D) {
                    uint32_t ump = (0x2 << 28) | (0 << 24) | (b1 << 16) | (b2 << 8);
                    if (g_endpointA) {
                        g_endpointA.SendSingleMessageWords(MidiClock::Now(), ump);
                    }
                } else if (cin == 0x0F) {
                    uint32_t ump = (0x1 << 28) | (0 << 24) | (b1 << 16);
                    if (g_endpointA) {
                        g_endpointA.SendSingleMessageWords(MidiClock::Now(), ump);
                    }
                }
            }
        }
        g_usb_active = false;
    });
}

bool VerifyWinMMReadiness(const std::wstring& targetPortName) {
    std::wcout << L">>> Verifying WinMM publication for \"" << targetPortName << L"\"..." << std::endl;

    for (int attempt = 0; attempt < 30; attempt++) {
        UINT inDevs = midiInGetNumDevs();
        int foundIn = -1;
        for (UINT i = 0; i < inDevs; i++) {
            MIDIINCAPSW caps = { 0 };
            if (midiInGetDevCapsW(i, &caps, sizeof(caps)) == MMSYSERR_NOERROR) {
                if (wcsstr(caps.szPname, targetPortName.c_str()) != nullptr) {
                    foundIn = (int)i;
                    break;
                }
            }
        }

        UINT outDevs = midiOutGetNumDevs();
        int foundOut = -1;
        for (UINT i = 0; i < outDevs; i++) {
            MIDIOUTCAPSW caps = { 0 };
            if (midiOutGetDevCapsW(i, &caps, sizeof(caps)) == MMSYSERR_NOERROR) {
                if (wcsstr(caps.szPname, targetPortName.c_str()) != nullptr) {
                    foundOut = (int)i;
                    break;
                }
            }
        }

        if (foundIn >= 0 && foundOut >= 0) {
            HMIDIIN hIn = NULL;
            MMRESULT inRes = midiInOpen(&hIn, foundIn, 0, 0, 0);
            if (inRes == MMSYSERR_NOERROR) {
                midiInClose(hIn);

                HMIDIOUT hOut = NULL;
                MMRESULT outRes = midiOutOpen(&hOut, foundOut, 0, 0, 0);
                if (outRes == MMSYSERR_NOERROR) {
                    midiOutClose(hOut);
                    std::wcout << L"  [WinMM Ready] Input Port:  \"" << targetPortName << L"\" (Dev #" << foundIn << L") -> OK" << std::endl;
                    std::wcout << L"  [WinMM Ready] Output Port: \"" << targetPortName << L"\" (Dev #" << foundOut << L") -> OK" << std::endl;
                    return true;
                }
            }
        }

        std::this_thread::sleep_for(std::chrono::milliseconds(200));
    }
    return false;
}

int main() {
    try {
        SetConsoleCtrlHandler(ConsoleCtrlHandler, TRUE);

        std::cout << "================================================================================" << std::endl;
        std::cout << " Roland Fantom-X Windows 11 WinUSB Bridge (v1.0)" << std::endl;
        std::cout << " Microsoft WinUSB + Windows MIDI Services | Verified with Memory Integrity (HVCI)" << std::endl;
        std::cout << "================================================================================" << std::endl;

        // Single-instance enforcement via named system mutex
        HANDLE hSingleInstanceMutex = CreateMutexW(NULL, TRUE, L"Local\\FantomXWinUsbBridgeMutex");
        if (hSingleInstanceMutex == NULL || GetLastError() == ERROR_ALREADY_EXISTS) {
            std::cout << "\n>>> [INFO] Roland Fantom-X Bridge is already running in another window." << std::endl;
            if (hSingleInstanceMutex != NULL) {
                CloseHandle(hSingleInstanceMutex);
            }
            return 0;
        }

        winrt::init_apartment();

        if (!MidiApi::EnsureServiceAvailable()) {
            std::cerr << "ERROR: Could not connect to Windows MIDI Service (MidiSrv)." << std::endl;
            if (hSingleInstanceMutex != NULL) CloseHandle(hSingleInstanceMutex);
            return 1;
        }

        std::cout << "\n>>> Initializing Windows MIDI A/B Loopback Endpoint Pair..." << std::endl;

        // Isolate loopback recovery: only clean up our own stale loopback pair if it exists from a past crash
        try {
            auto activeEntries = MidiLoopbackManager::GetActiveLoopbackEntries();
            for (auto entry : activeEntries) {
                if (entry.EndpointA().Name() == L"Fantom-X Bridge Host" || 
                    entry.EndpointB().Name() == L"Roland Fantom-X") {
                    MidiLoopbackRemovalConfig remConfig(entry.AssociationId());
                    MidiLoopbackManager::RemoveTransientLoopback(remConfig);
                    std::this_thread::sleep_for(std::chrono::milliseconds(200));
                }
            }
        } catch (...) {}

        MidiLoopbackEndpointDefinition definitionA;
        MidiLoopbackEndpointDefinition definitionB;

        definitionA.Name(L"Fantom-X Bridge Host");
        definitionA.Description(L"Internal Host Interface for fantomx-bridge");

        definitionB.Name(L"Roland Fantom-X");
        definitionB.Description(L"Roland Fantom-X DAW Interface");

        MidiLoopbackCreationConfig creationConfig(definitionA, definitionB);
        auto response = MidiLoopbackManager::CreateTransientLoopback(creationConfig);

        if (!response.Success()) {
            std::wcerr << L"Failed to create loopback pair: " << response.ErrorMessage().c_str() << std::endl;
            if (hSingleInstanceMutex != NULL) CloseHandle(hSingleInstanceMutex);
            return 1;
        }

        g_associationId = response.CreatedLoopbackEntry().AssociationId();
        std::wstring endpointAId = response.CreatedLoopbackEntry().EndpointA().EndpointDeviceId().c_str();

        std::wcout << L">>> Endpoint A (Host Bridge): " << response.CreatedLoopbackEntry().EndpointA().Name().c_str() << std::endl;
        std::wcout << L">>> Endpoint B (DAW Client):  " << response.CreatedLoopbackEntry().EndpointB().Name().c_str() << std::endl;

        auto session = MidiSession::Create(L"Fantom-X WinUSB Bridge");
        g_endpointA = session.CreateEndpointConnection(endpointAId);

        g_endpointA.MessageReceived([](const auto&, const MidiMessageReceivedEventArgs& args) {
            uint32_t word0 = args.PeekFirstWord();
            BYTE msgType = (word0 >> 28) & 0x0F;
            if (msgType == 0x2) {
                BYTE status = (word0 >> 16) & 0xFF;
                BYTE d1 = (word0 >> 8) & 0xFF;
                BYTE d2 = word0 & 0xFF;
                BYTE cin = (status >> 4) & 0x0F;
                BYTE packet[4] = { cin, status, d1, d2 };
                SendRawUsbMidi(packet, 4);
            } else if (msgType == 0x1) {
                BYTE status = (word0 >> 16) & 0xFF;
                BYTE packet[4] = { 0x0F, status, 0x00, 0x00 };
                SendRawUsbMidi(packet, 4);
            }
        });

        g_endpointA.Open();

        if (VerifyWinMMReadiness(L"Roland Fantom-X")) {
            std::wcout << L"\n>>> SUCCESS: \"Roland Fantom-X\" is fully published and ready for your DAW!" << std::endl;
            std::wcout << L">>> (In your DAW: Select \"Roland Fantom-X\" and enable MIDI input)\n" << std::endl;
        }

        auto guids = DiscoverFantomXGuids();
        std::cout << ">>> Bridge daemon active. Waiting for Fantom-X connection...\n" << std::endl;

        while (g_running) {
            if (!g_usb_active) {
                // If previous handles existed from an earlier disconnect, cleanly release them before re-opening
                CleanupUsbHandles();

                std::wstring path = FindAttachedDevicePath(guids);
                if (!path.empty() && g_running) {
                    std::lock_guard<std::mutex> lock(g_usb_mutex);
                    g_file_handle = CreateFileW(path.c_str(), GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE, NULL, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED, NULL);
                    if (g_file_handle != INVALID_HANDLE_VALUE) {
                        if (WinUsb_Initialize(g_file_handle, &g_winusb_handle)) {
                            UCHAR disable_suspend = 0;
                            WinUsb_SetPowerPolicy(g_winusb_handle, AUTO_SUSPEND, 1, &disable_suspend);
                            UCHAR allow_partial = 1;
                            WinUsb_SetPipePolicy(g_winusb_handle, 0x82, ALLOW_PARTIAL_READS, 1, &allow_partial);

                            g_usb_active = true;
                            std::wcout << L">>> FANTOM-X ATTACHED & STREAMING TO DAW! (Device: " << path << L")" << std::endl;
                            StartUsbReader(g_winusb_handle);
                        } else {
                            CloseHandle(g_file_handle);
                            g_file_handle = INVALID_HANDLE_VALUE;
                        }
                    }
                }
            }
            std::this_thread::sleep_for(std::chrono::milliseconds(10));
        }

        // Deterministic shutdown in main thread
        CleanupUsbHandles();
        if (g_associationId != winrt::guid{}) {
            try {
                MidiLoopbackRemovalConfig remConfig(g_associationId);
                MidiLoopbackManager::RemoveTransientLoopback(remConfig);
            } catch (...) {}
        }
        session.Close();
        if (hSingleInstanceMutex != NULL) {
            CloseHandle(hSingleInstanceMutex);
        }
        return 0;
    } catch (const winrt::hresult_error& ex) {
        std::wcerr << L"[EXCEPTION] WinRT HRESULT: " << ex.message().c_str() << std::endl;
        return 1;
    } catch (const std::exception& ex) {
        std::cerr << "[EXCEPTION] std: " << ex.what() << std::endl;
        return 1;
    } catch (...) {
        std::cerr << "[EXCEPTION] Unknown exception occurred." << std::endl;
        return 1;
    }
}
