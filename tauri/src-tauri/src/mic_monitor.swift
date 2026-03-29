/// Long-running mic monitor that uses CoreAudio HAL property listeners
/// to detect mic state changes instantly. Outputs "MIC_ON" or "MIC_OFF"
/// to stdout when the default input device's running state changes.
///
/// Usage: compile with `swiftc -O mic_monitor.swift -o mic_monitor`
/// The parent process reads stdout line-by-line.

import CoreAudio
import Foundation

// Disable output buffering so events reach the parent process immediately
setbuf(stdout, nil)

var currentDeviceID: AudioObjectID = 0
var listenerInstalled = false

/// Get the default input device ID
func getDefaultInputDevice() -> AudioObjectID {
    var deviceID = AudioObjectID(kAudioObjectSystemObject)
    var address = AudioObjectPropertyAddress(
        mSelector: kAudioHardwarePropertyDefaultInputDevice,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain
    )
    var size = UInt32(MemoryLayout<AudioObjectID>.size)
    let status = AudioObjectGetPropertyData(
        AudioObjectID(kAudioObjectSystemObject), &address, 0, nil, &size, &deviceID
    )
    return status == noErr ? deviceID : 0
}

/// Check if a device is currently running (mic in use)
func isDeviceRunning(_ deviceID: AudioObjectID) -> Bool {
    var isRunning: UInt32 = 0
    var address = AudioObjectPropertyAddress(
        mSelector: kAudioDevicePropertyDeviceIsRunningSomewhere,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain
    )
    var size = UInt32(MemoryLayout<UInt32>.size)
    let status = AudioObjectGetPropertyData(deviceID, &address, 0, nil, &size, &isRunning)
    return status == noErr && isRunning > 0
}

/// Callback when device running state changes
let runningCallback: AudioObjectPropertyListenerProc = {
    (_, _, _, _) -> OSStatus in
    let running = isDeviceRunning(currentDeviceID)
    print(running ? "MIC_ON" : "MIC_OFF")
    return noErr
}

/// Callback when default input device changes
let defaultDeviceCallback: AudioObjectPropertyListenerProc = {
    (_, _, _, _) -> OSStatus in
    // Remove listener from old device
    if listenerInstalled && currentDeviceID != 0 {
        var runAddr = AudioObjectPropertyAddress(
            mSelector: kAudioDevicePropertyDeviceIsRunningSomewhere,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        AudioObjectRemovePropertyListener(currentDeviceID, &runAddr, runningCallback, nil)
    }
    // Install listener on new device
    currentDeviceID = getDefaultInputDevice()
    if currentDeviceID != 0 {
        var runAddr = AudioObjectPropertyAddress(
            mSelector: kAudioDevicePropertyDeviceIsRunningSomewhere,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain
        )
        AudioObjectAddPropertyListener(currentDeviceID, &runAddr, runningCallback, nil)
        listenerInstalled = true
        // Emit current state
        let running = isDeviceRunning(currentDeviceID)
        print(running ? "MIC_ON" : "MIC_OFF")
    }
    return noErr
}

// Set up initial device listener
currentDeviceID = getDefaultInputDevice()
if currentDeviceID != 0 {
    var runAddr = AudioObjectPropertyAddress(
        mSelector: kAudioDevicePropertyDeviceIsRunningSomewhere,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain
    )
    AudioObjectAddPropertyListener(currentDeviceID, &runAddr, runningCallback, nil)
    listenerInstalled = true

    // Emit initial state
    let running = isDeviceRunning(currentDeviceID)
    print(running ? "MIC_ON" : "MIC_OFF")
}

// Listen for default device changes (e.g., plugging in headphones)
var defaultAddr = AudioObjectPropertyAddress(
    mSelector: kAudioHardwarePropertyDefaultInputDevice,
    mScope: kAudioObjectPropertyScopeGlobal,
    mElement: kAudioObjectPropertyElementMain
)
AudioObjectAddPropertyListener(
    AudioObjectID(kAudioObjectSystemObject), &defaultAddr, defaultDeviceCallback, nil
)

// Print ready marker so Rust knows the monitor is running
print("READY")

// Run forever — parent process will kill us when it exits
RunLoop.main.run()
