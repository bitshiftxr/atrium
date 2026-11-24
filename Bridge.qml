import QtQuick
import Quickshell.Io

QtObject {
  id: root

  property string daemonPath: ""
  property bool started: false
  property bool failed: false
  property string lastError: ""

  // bin/atriumd-launcher reports why it could not start the daemon (a failed
  // download, a binary that does not match its pin) on a stderr line with this
  // prefix. It is kept per run and published when the run ends, so the reason
  // the panel shows does not flicker while a retry is under way.
  readonly property string launcherPrefix: "atriumd-launcher: "
  property string launcherError: ""
  property string runLauncherError: ""

  // Runs in a row that ended before the first event. Each retry may be a
  // download, so the wait between them doubles, up to five minutes.
  property int failedRuns: 0

  // A run that never says anything is a run that did not work. Spawning
  // succeeds for a binary that then dies on startup, so onStarted alone proves
  // nothing; the first event does.
  property bool sawEvent: false

  signal event(var payload)

  property var pending: []

  function start() {
    if (proc.running || daemonPath === "") return
    proc.command = [daemonPath, "serve"]
    runLauncherError = ""
    proc.running = true
    sawEvent = false
    startWatchdog.restart()
  }

  function stop() {
    pending = []
    started = false
    proc.running = false
  }

  function send(command) {
    var line = JSON.stringify(command)
    if (!started) {
      var queue = pending.slice()
      queue.push(line)
      pending = queue
      start()
      return
    }
    proc.write(line + "\n")
  }

  function flush() {
    var queue = pending
    pending = []
    for (var i = 0; i < queue.length; i++) proc.write(queue[i] + "\n")
  }

  function receive(line) {
    if (line === "") return
    var payload
    try {
      payload = JSON.parse(line)
    } catch (e) {
      root.lastError = "unreadable line from atriumd"
      return
    }
    if (!payload || typeof payload !== "object") return
    if (!root.sawEvent) {
      root.sawEvent = true
      root.failedRuns = 0
      root.launcherError = ""
      root.failed = false
    }
    root.event(payload)
  }

  property Process proc: Process {
    id: proc
    stdinEnabled: true
    onStarted: {
      startWatchdog.stop()
      root.started = true
      root.flush()
    }
    onExited: function(code, status) {
      root.started = false
      if (code !== 0) root.lastError = "atriumd exited with status " + code
      if (!root.sawEvent) {
        root.failedRuns++
        root.launcherError = root.runLauncherError
        root.failed = true
      }
      restartTimer.interval = Math.min(2000 * Math.pow(2, Math.max(0, root.failedRuns - 1)), 300000)
      restartTimer.restart()
    }
    stdout: SplitParser { onRead: function(line) { root.receive(line) } }
    stderr: SplitParser {
      onRead: function(line) {
        root.lastError = line
        if (line.indexOf(root.launcherPrefix) === 0)
          root.runLauncherError = line.slice(root.launcherPrefix.length)
      }
    }
  }

  property Timer restartTimer: Timer {
    id: restartTimer
    interval: 2000
    repeat: false
    onTriggered: root.start()
  }

  // A binary that is not there emits neither started nor exited — Process just
  // drops running back to false — so onExited never arms restartTimer and
  // nothing else here would ever look again. This timer is both halves: it
  // reports the failure, and it keeps trying, so building the daemon while the
  // shell is up fixes the panel without a reload. Spawning is local and takes
  // milliseconds, so silence this long means it did not start.
  property Timer startWatchdog: Timer {
    id: startWatchdog
    interval: 5000
    repeat: true
    onTriggered: {
      root.failed = true
      root.start()
    }
  }
}
