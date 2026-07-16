import QtQuick
import QtQuick.Controls
import StitchShell

FocusScope {
    id: root
    objectName: "editorRoot"
    width: 960
    height: 540
    focus: true

    Shortcut {
        sequences: [ StandardKey.Open ]
        onActivated: shell.openSnapshot()
    }
    Shortcut {
        sequence: "Ctrl+Return"
        onActivated: shell.submitDemoBatch()
    }
    Shortcut {
        sequence: "Escape"
        onActivated: shell.cancelEpoch()
    }

    Column {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 8
        Row {
            id: controls
            spacing: 8
            Button {
                id: openButton
                objectName: "openSnapshotButton"
                text: "Open snapshot"
                focusPolicy: Qt.StrongFocus
                Accessible.name: text
                Accessible.description: "Open the current deterministic project snapshot"
                onClicked: shell.openSnapshot()
            }
            Button {
                objectName: "submitEditButton"
                text: "Submit edit"
                focusPolicy: Qt.StrongFocus
                Accessible.name: text
                Accessible.description: "Add one typed track command"
                onClicked: shell.submitDemoBatch()
            }
            Button {
                objectName: "cancelPreviewButton"
                text: "Cancel preview"
                focusPolicy: Qt.StrongFocus
                Accessible.name: text
                Accessible.description: "Advance the preview cancellation epoch"
                onClicked: shell.cancelEpoch()
            }
            Label {
                text: shell.status
                Accessible.name: "Editor status: " + text
            }
        }
        PreviewItem {
            id: preview
            objectName: "nativePreview"
            width: parent.width
            height: Math.max(0, parent.height - controls.height - parent.spacing)
            activeFocusOnTab: true
            Accessible.name: "Synthetic native preview"
            Accessible.description: "Same-device Metal texture preview"
            Accessible.role: Accessible.Graphic
            Keys.onSpacePressed: shell.submitDemoBatch()
        }
    }
}
