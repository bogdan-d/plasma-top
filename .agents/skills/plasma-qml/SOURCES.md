# Sources and provenance

Primary sources:

- KDE Plasma widget setup: <https://develop.kde.org/docs/plasma/widget/setup/>
- KDE widget properties: <https://develop.kde.org/docs/plasma/widget/properties/>
- KDE Plasma QML API: <https://develop.kde.org/docs/plasma/widget/plasma-qml-api/>
- KDE Plasma widget testing: <https://develop.kde.org/docs/plasma/widget/testing/>
- KDE KF6 plasmoid porting: <https://develop.kde.org/docs/plasma/widget/porting_kf6/>
- Qt QML reference: <https://doc.qt.io/qt-6/qtqml-index.html>
- Qt `qmllint`: <https://doc.qt.io/qt-6/qtqml-tooling-qmllint.html>
- Qt QML language server: <https://doc.qt.io/qt-6/qtqml-tooling-qmlls.html>

Project authorities:

- `AGENTS.md`
- `docs/DESIGN.md`
- `docs/LAYOUT.md`
- `docs/PERFORMANCE.md`
- `docs/DEVELOPMENT.md`
- `plasmoid/package/contents/ui/main.qml`
- `tools/qml_verify.sh`
- `tools/p6_qt_matrix.sh`

The Qt Company R&D `agent-skills` repository was safety-reviewed at commit
`71d6c10da78b9a764468ae11c86ab3bc4ca4921f`. No upstream executable or skill
was vendored. Applicable ideas were restated here because its generic Qt app,
CMake, test-runner, and multi-agent assumptions do not match this plasmoid.

This skill contains Markdown only. It installs nothing, executes nothing, and
does not configure network-connected tools.
