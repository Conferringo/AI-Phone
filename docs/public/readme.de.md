# AI Phone

[English](../../README.md) · [Alle Sprachen](../../README.md) · Apache-2.0

## Downloads und Stand

Vollständige Installationspakete stehen noch nicht zum Download bereit. Die Einrichtung folgt dem Ablauf Geräteressourcen → Modell → Arbeitsbereich. Ein vollständiges Paket enthält Android/Linux-Basissysteme, Emulator und vorkompilierte Dienste. Der Client richtet die Umgebung ein; Benutzer müssen keine Images suchen oder Backend-Dienste kompilieren.

Build-Ziele sind macOS Apple Silicon, Windows x64, Linux x64/ARM64 und Android ARM64. Android-Hosts benötigen Root und KVM. Paketverfügbarkeit und tatsächlich geprüfte Plattformen stehen im Veröffentlichungsstatus.

## Funktionen und Szenarien

Ein echter Android-Gast und ein schlanker Linux-Desktop teilen sich eine Arbeitsoberfläche. Standardmäßig gibt es ein Telefon und einen Computer, mit eigenen Gesprächen und gemeinsamem oder getrenntem Agenten.

- Apps wie WeChat anhand realer Bildschirme und Benachrichtigungen bedienen und im Auftrag des Nutzers antworten. Konto- und App-Beschränkungen bleiben wirksam.
- Eine Datei am Telefon empfangen, an Linux übertragen, dort eine Tabelle bearbeiten und prüfen und das Ergebnis zurückgeben. Geräteübertragung und Zustellung an einen Kontakt werden getrennt geprüft.
- Laufende Aufgaben durch weitere Nachrichten lenken und unterbrechen, ohne frühere Nachrichten oder Aktionen zu verlieren. Benachrichtigungen können den Agenten wecken; autonome Erkundung lässt sich pro Gerät pausieren.
- Skills, verknüpfte Wissensdokumente und Arbeitsdateien gemeinsam nutzen. Modelle/APIs, Gedächtnis und externe Verbindungen gehören zum Agenten. Clash läuft als Anwendung in den Gästen; der Host nutzt standardmäßig Direktverbindungen.

Aktionen sind an Beobachtung, Gerät, Aufgabe, Kontroll-Epoche und eindeutige Aktions-ID gebunden. Unbekannte Ergebnisse werden vor Wiederholung beobachtet. Modelle können Oberflächen weiterhin falsch interpretieren; Ausführungsprotokolle garantieren keine Fehlerfreiheit.

## Technik und Daten

`apps/web`: Next.js App Router, React, Tailwind und shadcn. `apps/client`: Tauri 2. `crates`: Rust/Axum/Tokio. `packages/protocol`: protobuf. PostgreSQL/SQLx speichert Zustand; Redis ist für optionales Caching vorgesehen, nicht als Aufgabenwarteschlange. Bun und Cargo bauen das Projekt, Compose betreibt Dienste. `infra`, `scripts`, `skills` und `config` enthalten Laufzeit und integrierte Fähigkeiten.

Code und Nutzerdaten sind getrennt. Geräteplatten, Konten, Chats, Anhänge und Schlüssel werden nicht veröffentlicht. Updates benötigen versionierte Pakete, Sicherung, Migration und Rücknahme statt eines direkten `git pull` in Nutzerdaten.

## Ausblick, Verantwortung und Lizenz

Prioritäten: vollständige Pakete, Wiederaufnahme der Installation, signierte Updates, Agentenstabilität, Audio/Eingabe, Modellprüfung und spezialisierte App-Skills. Sprache und experimentelle Telefonie folgen später; echte Anrufe/eSIM sind nicht implementiert.

Oberfläche und Ersteinrichtung unterstützen zehn Sprachen. Der Sprachschalter links von Skills ändert die Oberfläche; Gastbetriebssysteme und rohe Werkzeugausgaben werden nicht automatisch übersetzt. Arabisch verwendet RTL.

Nutzer verantworten Berechtigungen, Konten, externe Aktionen und App-Bedingungen. Kein Einsatz für Täuschung, Betrug oder Handlungen ohne Einwilligung. Originalcode: Apache-2.0; Drittkomponenten behalten ihre Lizenzen. Der Hinweis ersetzt keine rechtliche Verantwortung.

[Freigabestand](release-status.md) · [Architektur](architecture.en.md) · [Kompatibilität](compatibility.md) · [Drittanbieter](../../THIRD_PARTY_NOTICES.md) · [Haftungshinweise](../../DISCLAIMER.md)
