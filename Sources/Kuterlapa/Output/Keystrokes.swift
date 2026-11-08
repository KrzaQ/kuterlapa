import ApplicationServices
import Foundation

/// A key with modifiers, for example "cmd+shift+d" or "Return".
struct KeyCombo: Equatable {
    let code: CGKeyCode
    let flags: CGEventFlags

    init?(_ spec: String) {
        var flags: CGEventFlags = []
        var code: CGKeyCode?
        for part in spec.lowercased().split(separator: "+").map(String.init) {
            if let flag = Self.modifiers[part] {
                flags.insert(flag)
            } else if let c = Self.keyCodes[part], code == nil {
                code = c
            } else {
                return nil
            }
        }
        guard let code else { return nil }
        self.code = code
        self.flags = flags
    }

    static let modifiers: [String: CGEventFlags] = [
        "cmd": .maskCommand, "command": .maskCommand,
        "ctrl": .maskControl, "control": .maskControl,
        "alt": .maskAlternate, "option": .maskAlternate, "opt": .maskAlternate,
        "shift": .maskShift,
    ]

    /// Virtual key codes of the ANSI layout (Carbon kVK_* constants).
    static let keyCodes: [String: CGKeyCode] = [
        "return": 0x24, "enter": 0x24, "tab": 0x30, "space": 0x31,
        "delete": 0x33, "backspace": 0x33, "escape": 0x35, "esc": 0x35,
        "left": 0x7B, "right": 0x7C, "down": 0x7D, "up": 0x7E,
        "pageup": 0x74, "pagedown": 0x79, "home": 0x73, "end": 0x77,
        "f1": 0x7A, "f2": 0x78, "f3": 0x63, "f4": 0x76, "f5": 0x60, "f6": 0x61,
        "f7": 0x62, "f8": 0x64, "f9": 0x65, "f10": 0x6D, "f11": 0x67, "f12": 0x6F,
        "f13": 0x69, "f14": 0x6B, "f15": 0x71, "f16": 0x6A, "f17": 0x40,
        "f18": 0x4F, "f19": 0x50, "f20": 0x5A,
        "a": 0x00, "s": 0x01, "d": 0x02, "f": 0x03, "h": 0x04, "g": 0x05,
        "z": 0x06, "x": 0x07, "c": 0x08, "v": 0x09, "b": 0x0B, "q": 0x0C,
        "w": 0x0D, "e": 0x0E, "r": 0x0F, "y": 0x10, "t": 0x11, "o": 0x1F,
        "u": 0x20, "i": 0x22, "p": 0x23, "l": 0x25, "j": 0x26, "k": 0x28,
        "n": 0x2D, "m": 0x2E,
        "1": 0x12, "2": 0x13, "3": 0x14, "4": 0x15, "5": 0x17, "6": 0x16,
        "7": 0x1A, "8": 0x1C, "9": 0x19, "0": 0x1D,
        // "+" separates the parts, so the plus key is "=" (cmd+= zooms in).
        "=": 0x18, "equal": 0x18, "-": 0x1B, "minus": 0x1B,
        "[": 0x21, "]": 0x1E, ";": 0x29, "'": 0x27, ",": 0x2B, ".": 0x2F, "/": 0x2C,
        "\\": 0x2A, "`": 0x32,
    ]
}

/// Sends keystrokes and text to the frontmost application.
/// Needs the macOS Accessibility permission.
enum Keystrokes {
    static var allowed: Bool { AXIsProcessTrusted() }

    /// Shows the macOS dialog that asks for the Accessibility permission,
    /// if the app does not have it yet.
    @discardableResult
    static func requestPermission() -> Bool {
        let key = kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String
        return AXIsProcessTrustedWithOptions([key: true] as CFDictionary)
    }

    static func press(_ combo: KeyCombo) {
        let source = CGEventSource(stateID: .hidSystemState)
        for down in [true, false] {
            let event = CGEvent(keyboardEventSource: source, virtualKey: combo.code, keyDown: down)
            event?.flags = combo.flags
            event?.post(tap: .cghidEventTap)
        }
    }

    /// Types text as Unicode, so it does not depend on the keyboard layout.
    /// One event carries at most 20 UTF-16 units, so the text goes in chunks.
    static func type(_ text: String) {
        let source = CGEventSource(stateID: .hidSystemState)
        let units = Array(text.utf16)
        var start = 0
        while start < units.count {
            var chunk = Array(units[start..<min(start + 20, units.count)])
            for down in [true, false] {
                let event = CGEvent(keyboardEventSource: source, virtualKey: 0, keyDown: down)
                event?.keyboardSetUnicodeString(stringLength: chunk.count, unicodeString: &chunk)
                event?.post(tap: .cghidEventTap)
            }
            start += 20
            usleep(2000)
        }
    }
}
