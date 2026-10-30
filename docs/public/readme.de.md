# AI Phone

[English](../../README.md) · [Alle Sprachen](../../README.md) · Apache-2.0

## Downloads und Stand

Ein vollständiges Produktinstallationspaket ist noch nicht verfügbar. Unvollständige Vorschaupakete wurden zurückgezogen. Dieses Repository enthält Entwicklungscode. Der nächste Vertrieb muss saubere Android-/Linux-Abbilder, Emulator und vorgefertigte Dienste enthalten. Nutzer sollen Geräte und Modell im Client konfigurieren können, ohne Abbilder zu suchen, ein SDK einzurichten oder den Server zu kompilieren.

Zielplattformen: macOS Apple Silicon, Windows x64, Linux x64/ARM64 und Android ARM64. macOS und Linux x64 besitzen experimentelle Entwicklungs-Bootstraps. Windows als eigenständiger Host und die vollständige Linux-ARM64-Kette sind offen. Android als Host setzt root und verfügbares KVM voraus; eine gewöhnliche APK ohne root erfüllt das nicht. Vollständige eigenständige Pakete sind auf keiner Plattform abgenommen.

## Funktionen und Szenarien

Ein echter Android-Gast und ein schlanker Linux-Desktop teilen sich eine Arbeitsoberfläche. Standardmäßig gibt es ein Telefon und einen Computer, mit eigenen Gesprächen und gemeinsamem oder getrenntem Agenten.

- Apps wie WeChat anhand realer Bildschirme und Benachrichtigungen bedienen und im Auftrag des Nutzers antworten. Konto- und App-Beschränkungen bleiben wirksam.
- Eine Datei am Telefon empfangen, an Linux übertragen, dort eine Tabelle bearbeiten und prüfen und das Ergebnis zurückgeben. Geräteübertragung und Zustellung an einen Kontakt werden getrennt geprüft.
- Laufende Aufgaben durch weitere Nachrichten lenken und unterbrechen, ohne frühere Nachrichten oder Aktionen zu verlieren. Benachrichtigungen können den Agenten wecken; autonome Erkundung lässt sich pro Gerät pausieren.
- Skills, verknüpfte Wissensdokumente und Arbeitsdateien gemeinsam nutzen. Modelle/APIs, Gedächtnis und externe Verbindungen gehören zum Agenten. Clash läuft als Anwendung in den Gästen; der Host nutzt standardmäßig Direktverbindungen.

Aktionen sind an Beobachtung, Gerät, Aufgabe, Kontroll-Epoche und eindeutige Aktions-ID gebunden. Unbekannte Ergebnisse werden vor Wiederholung beobachtet. Modelle können Oberflächen weiterhin falsch interpretieren; Ausführungsprotokolle garantieren keine Fehlerfreiheit.

## Technik und Daten

`apps/web`: Next.js App Router, React, Tailwind und shadcn. `apps/client`: Tauri 2. `crates`: Rust/Axum/Tokio. `packages/protocol`: protobuf. PostgreSQL/SQLx speichert Zustand; Redis dient temporärer Koordination. Bun und Cargo bauen das Projekt, Compose betreibt Dienste. `infra`, `scripts`, `skills` und `config` enthalten Laufzeit und integrierte Fähigkeiten.

Code und Nutzerdaten sind getrennt. Geräteplatten, Konten, Chats, Anhänge und Schlüssel werden nicht veröffentlicht. Updates benötigen versionierte Pakete, Sicherung, Migration und Rücknahme statt eines direkten `git pull` in Nutzerdaten.

## Ausblick, Verantwortung und Lizenz

Prioritäten: vollständige Pakete, Wiederaufnahme der Installation, signierte Updates, Agentenstabilität, Audio/Eingabe, Modellprüfung und spezialisierte App-Skills. Sprache und experimentelle Telefonie folgen später; echte Anrufe/eSIM sind nicht implementiert. Das Desktop-Haustier wurde aus dem Kern entfernt und bleibt nur eine Erweiterungsidee.

Oberfläche und Ersteinrichtung unterstützen zehn Sprachen. Der Sprachschalter links von Skills ändert die Oberfläche; Gastbetriebssysteme und rohe Werkzeugausgaben werden nicht automatisch übersetzt. Arabisch verwendet RTL.

Nutzer verantworten Berechtigungen, Konten, externe Aktionen und App-Bedingungen. Kein Einsatz für Täuschung, Betrug oder Handlungen ohne Einwilligung. Originalcode: Apache-2.0; Drittkomponenten behalten ihre Lizenzen. Der Hinweis ersetzt keine rechtliche Verantwortung.

[Freigabestand](release-status.md) · [Architektur](architecture.en.md) · [Kompatibilität](compatibility.md) · [Drittanbieter](../../THIRD_PARTY_NOTICES.md) · [Haftungshinweise](../../DISCLAIMER.md)
