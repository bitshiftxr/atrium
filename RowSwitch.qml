import QtQuick
import qs.Ui
import qs.Commons

// The on/off switch drawn on every device row, and on the room picker for the
// whole room. One component so the two can never drift apart: same size, same
// theme colours, same pulse while an action is in flight.
Rectangle {
  id: root

  property bool on: false
  property bool pending: false

  signal clicked()

  width: Style.space(26)
  height: Style.space(14)
  radius: Style.cornerRadius
  color: "transparent"
  border.width: 1
  border.color: root.on ? Color.accent : Style.normalBorderColor

  Rectangle {
    width: Style.space(10)
    height: Style.space(10)
    radius: Style.cornerRadius
    anchors.verticalCenter: parent.verticalCenter
    anchors.left: root.on ? undefined : parent.left
    anchors.right: root.on ? parent.right : undefined
    anchors.leftMargin: 1
    anchors.rightMargin: 1
    color: root.on ? Color.accent : Color.muted

    SequentialAnimation on opacity {
      running: root.pending
      loops: Animation.Infinite
      NumberAnimation { to: 0.3; duration: 400 }
      NumberAnimation { to: 1.0; duration: 400 }
    }
  }

  MouseArea {
    anchors.fill: parent
    cursorShape: Qt.PointingHandCursor
    onClicked: root.clicked()
  }
}
