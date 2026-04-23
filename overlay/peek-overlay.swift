import Cocoa
import SwiftUI

// MARK: - Protocol Types

struct OverlayCommand: Codable {
    let action: String
    let items: [SuggestionItem]?
    let selected: Int?
}

struct SuggestionItem: Codable, Identifiable {
    let name: String
    let preview: String
    var id: String { name }
}

// MARK: - Overlay State

class OverlayState: ObservableObject {
    @Published var items: [SuggestionItem] = []
    @Published var selectedIndex: Int = 0
    @Published var isVisible: Bool = false
}

// MARK: - Dropdown View

struct DropdownView: View {
    @ObservedObject var state: OverlayState

    var body: some View {
        VStack(spacing: 0) {
            ForEach(Array(state.items.prefix(8).enumerated()), id: \.element.id) { index, item in
                HStack(spacing: 0) {
                    Text(item.name)
                        .font(.system(size: 13, weight: index == state.selectedIndex ? .semibold : .regular, design: .monospaced))
                        .foregroundColor(index == state.selectedIndex ? .white : .primary)
                        .frame(maxWidth: 180, alignment: .leading)
                        .lineLimit(1)

                    Spacer(minLength: 12)

                    Text(item.preview)
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundColor(index == state.selectedIndex ? .white.opacity(0.7) : .secondary)
                        .frame(maxWidth: 220, alignment: .trailing)
                        .lineLimit(1)
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 5)
                .background(
                    RoundedRectangle(cornerRadius: 5)
                        .fill(index == state.selectedIndex ? Color.accentColor : Color.clear)
                )
                .padding(.horizontal, 4)
            }
        }
        .padding(.vertical, 6)
        .frame(width: 440)
        .background(
            VisualEffectView(material: .hudWindow, blendingMode: .behindWindow)
                .clipShape(RoundedRectangle(cornerRadius: 10))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(Color.primary.opacity(0.1), lineWidth: 0.5)
        )
        .shadow(color: .black.opacity(0.3), radius: 12, y: 4)
    }
}

// MARK: - NSVisualEffectView wrapper

struct VisualEffectView: NSViewRepresentable {
    let material: NSVisualEffectView.Material
    let blendingMode: NSVisualEffectView.BlendingMode

    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.material = material
        view.blendingMode = blendingMode
        view.state = .active
        return view
    }

    func updateNSView(_ nsView: NSVisualEffectView, context: Context) {
        nsView.material = material
        nsView.blendingMode = blendingMode
    }
}

// MARK: - Overlay Window

class OverlayPanel: NSPanel {
    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}

class OverlayWindowController {
    let panel: OverlayPanel
    let state: OverlayState
    var hostingView: NSHostingView<DropdownView>?

    init() {
        state = OverlayState()

        panel = OverlayPanel(
            contentRect: NSRect(x: 0, y: 0, width: 440, height: 300),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: true
        )
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false // SwiftUI handles shadow
        panel.level = .floating
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        panel.hidesOnDeactivate = false
        panel.isMovableByWindowBackground = false

        let hosting = NSHostingView(rootView: DropdownView(state: state))
        hosting.frame = panel.contentView!.bounds
        hosting.autoresizingMask = [.width, .height]
        panel.contentView?.addSubview(hosting)
        hostingView = hosting
    }

    func show(items: [SuggestionItem], selected: Int) {
        DispatchQueue.main.async { [self] in
            state.items = items
            state.selectedIndex = selected

            // Calculate height based on item count
            let itemCount = min(items.count, 8)
            let itemHeight: CGFloat = 28
            let padding: CGFloat = 12
            let height = CGFloat(itemCount) * itemHeight + padding

            // Position near the frontmost terminal window's cursor
            positionNearCursor(height: height)

            panel.setContentSize(NSSize(width: 440, height: height))
            panel.orderFrontRegardless()
            state.isVisible = true
        }
    }

    func updateSelection(_ index: Int) {
        DispatchQueue.main.async { [self] in
            state.selectedIndex = index
        }
    }

    func hide() {
        DispatchQueue.main.async { [self] in
            panel.orderOut(nil)
            state.isVisible = false
            state.items = []
        }
    }

    private func positionNearCursor(height: CGFloat) {
        // Find the frontmost terminal window and position below it
        guard let screen = NSScreen.main else { return }

        // Get frontmost app's window position via CGWindowList
        if let windowInfo = getTerminalWindowFrame() {
            // Position at bottom-left area of the terminal window
            // The y-coordinate from CG is top-down, but NSWindow uses bottom-up
            let screenHeight = screen.frame.height
            let windowBottom = screenHeight - windowInfo.maxY
            let x = windowInfo.minX + 20
            let y = windowBottom + 40 // Just above the bottom of terminal

            panel.setFrameOrigin(NSPoint(x: x, y: y))
        } else {
            // Fallback: center bottom of screen
            let screenFrame = screen.visibleFrame
            let x = screenFrame.midX - 220
            let y = screenFrame.minY + 60
            panel.setFrameOrigin(NSPoint(x: x, y: y))
        }
    }

    private func getTerminalWindowFrame() -> CGRect? {
        let options = CGWindowListOption([.optionOnScreenOnly, .excludeDesktopElements])
        guard let windowList = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] else {
            return nil
        }

        // Find terminal windows (common terminal apps)
        let terminalBundleIds = [
            "com.mitchellh.ghostty",
            "com.googlecode.iterm2",
            "com.apple.Terminal",
            "io.alacritty",
            "net.kovidgoyal.kitty",
            "dev.warp.Warp-Stable",
            "com.github.wez.wezterm",
        ]

        for window in windowList {
            guard let ownerName = window[kCGWindowOwnerName as String] as? String,
                  let bounds = window[kCGWindowBounds as String] as? [String: CGFloat],
                  let layer = window[kCGWindowLayer as String] as? Int,
                  layer == 0 else { continue }

            // Check if it's a terminal by bundle ID or name
            let bundleId = window["kCGWindowOwnerBundleIdentifier" as String] as? String ?? ""
            let isTerminal = terminalBundleIds.contains(bundleId) ||
                           ownerName.lowercased().contains("terminal") ||
                           ownerName.lowercased().contains("iterm") ||
                           ownerName.lowercased().contains("ghostty")

            if isTerminal {
                let x = bounds["X"] ?? 0
                let y = bounds["Y"] ?? 0
                let w = bounds["Width"] ?? 800
                let h = bounds["Height"] ?? 600
                return CGRect(x: x, y: y, width: w, height: h)
            }
        }

        return nil
    }
}

// MARK: - Stdin Reader

class StdinReader {
    let controller: OverlayWindowController

    init(controller: OverlayWindowController) {
        self.controller = controller
    }

    func start() {
        DispatchQueue.global(qos: .userInteractive).async { [self] in
            let handle = FileHandle.standardInput
            var buffer = Data()

            while true {
                let data = handle.availableData
                if data.isEmpty {
                    // EOF — parent process closed the pipe
                    DispatchQueue.main.async {
                        NSApplication.shared.terminate(nil)
                    }
                    break
                }

                buffer.append(data)

                // Process complete lines
                while let newlineRange = buffer.range(of: Data([0x0a])) {
                    let lineData = buffer.subdata(in: buffer.startIndex..<newlineRange.lowerBound)
                    buffer.removeSubrange(buffer.startIndex...newlineRange.lowerBound)

                    if let line = String(data: lineData, encoding: .utf8),
                       !line.isEmpty {
                        processCommand(line)
                    }
                }
            }
        }
    }

    private func processCommand(_ json: String) {
        guard let data = json.data(using: .utf8),
              let command = try? JSONDecoder().decode(OverlayCommand.self, from: data) else {
            return
        }

        switch command.action {
        case "show":
            if let items = command.items {
                controller.show(items: items, selected: command.selected ?? 0)
            }
        case "update":
            if let selected = command.selected {
                controller.updateSelection(selected)
            }
        case "hide":
            controller.hide()
        default:
            break
        }
    }
}

// MARK: - App Delegate

class AppDelegate: NSObject, NSApplicationDelegate {
    var controller: OverlayWindowController!
    var reader: StdinReader!

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Hide from dock and cmd-tab
        NSApp.setActivationPolicy(.accessory)

        controller = OverlayWindowController()
        reader = StdinReader(controller: controller)
        reader.start()

        // Hide overlay when user switches to another app
        NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didActivateApplicationNotification,
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard let app = notification.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication else { return }
            // If the newly activated app is NOT a terminal, hide the overlay
            let terminalBundleIds = [
                "com.mitchellh.ghostty",
                "com.googlecode.iterm2",
                "com.apple.Terminal",
                "io.alacritty",
                "net.kovidgoyal.kitty",
                "dev.warp.Warp-Stable",
                "com.github.wez.wezterm",
            ]
            if let bundleId = app.bundleIdentifier,
               !terminalBundleIds.contains(bundleId) {
                self?.controller.hide()
            }
        }
    }
}

// MARK: - Main

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.run()
