# Prompt dla Claude Code — CD Ripper (Tauri + Nuxt)

---

## Kontekst projektu

Budujemy od zera desktopową aplikację do zgrywania (ripowania) płyt CD Audio, w pełni open source, działającą na **Windows, Linux i macOS**. Rynek ma tu realną lukę: EAC i CUERipper są dokładne, ale są tylko na Windows, mają archaiczne GUI i nie pobierają tekstów piosenek; cyanrip jest cross-platform i dokładny, ale to CLI bez tekstów; MusicBee ma ładne GUI, ale słaby rdzeń ripujący i też tylko Windows. Chcemy jedno narzędzie, które w jednym przebiegu daje: bit-perfect rip z weryfikacją, komplet metadanych, okładkę, gatunek, poprawne numery płyt w wydaniach wielopłytowych i teksty piosenek.

Użytkownik wybiera format wyjściowy — **FLAC, WAV, AIFF, ALAC, APE, MP3, AAC, Ogg Vorbis, M4A** — i może zaznaczyć kilka naraz. Płyta jest wtedy czytana raz, a enkodowanie do wszystkich wybranych formatów leci równolegle z odczytu.

**Windows, Linux i macOS są równorzędnymi celami od pierwszego commita.** Nie ma platformy „głównej" i nie odkładamy przenośności na później — dopisywanie wsparcia po fakcie oznaczałoby przepisanie warstwy dostępu do napędu i całego buildu.

Konsekwencje:

- Kod specyficzny dla systemu żyje wyłącznie w module `drive`, za wspólnym traitem. Nigdzie indziej nie ma `#[cfg(target_os)]`.
- Żadnych zahardkodowanych ścieżek — katalogi konfiguracji, cache i muzyki z API ścieżek Tauri albo z crate'a `directories`.
- Każda nowa zależność musi budować się na wszystkich trzech systemach.
- Konfiguracja CI (GitHub Actions, matrix na trzech systemach) jest w repo od początku. Commituj ją normalnie — po prostu nigdy nie pushuj.

---

## Twarde zasady — obowiązują przez cały czas pracy

### Git

- Commituj często, małymi logicznymi krokami. Każdy commit musi zostawiać repo w stanie, który się kompiluje.
- **Nigdy nie rób `git push`.** Ani razu, pod żadnym pretekstem. Tylko lokalne commity.
- Nie twórz PR-ów, nie konfiguruj remote'ów, nie ruszaj `gh`.
- **Wiadomości commitów: żadnych śladów AI.** Konkretnie zabronione:
  - trailer `Co-Authored-By: Claude ...` lub jakikolwiek inny co-author
  - stopki typu `Generated with Claude Code`, linki do anthropic.com
  - emoji w wiadomościach commitów
  - zwroty typu „as an AI", „I've implemented" — commity pisz w trybie rozkazującym, bezosobowo
- Format: Conventional Commits, tryb rozkazujący, po angielsku, temat do 72 znaków. Przykłady:
  - `feat(toc): read raw TOC via IOCTL_CDROM_READ_TOC_EX`
  - `fix(paranoia): handle READERR status without aborting rip`
  - `refactor(metadata): extract MusicBrainz client into own module`
- Body commita tylko wtedy, gdy wyjaśnia *dlaczego*, nie *co*.
- W kodzie, komentarzach, docstringach i dokumentacji też nie zostawiaj śladów, że pisało to AI. Komentarze pisz jak człowiek: rzadko, tylko tam gdzie kod jest nieoczywisty, i wyjaśniaj powód, nie treść linijki.

### Zależności i wersje

- Menedżer pakietów JS to **wyłącznie pnpm**. Nigdy npm ani yarn. Nie commituj `package-lock.json` ani `yarn.lock`.
- **Nie zgaduj numerów wersji z pamięci.** Zanim dodasz jakąkolwiek zależność, sprawdź aktualną wersję:
  - JS: `pnpm view <pakiet> version` (albo instaluj przez `pnpm add <pakiet>@latest`)
  - Rust: `cargo add <crate>` samo rozwiąże najnowszą — nie wpisuj wersji ręcznie do `Cargo.toml`
  - W razie wątpliwości sprawdź stronę pakietu (npm / crates.io) i changelog pod kątem breaking changes
- Po instalacji sprawdź, czy nie ma konfliktu peer dependencies i czy build przechodzi.
- Preferuj mniej zależności. Jeśli coś da się zrobić w 30 liniach zamiast dociągać crate — pisz sam.

### Skille

Zanim zaczniesz pracę nad daną częścią kodu, sprawdź dostępne skille i użyj tych, które pasują do zadania — nie zakładaj z góry, że wiesz jak coś zrobić w tym środowisku. W szczególności przed pisaniem czy przeprojektowywaniem jakiegokolwiek UI (komponenty Nuxt/Vue, style, layout, typografia) sięgnij po skill od frontend designu. Jeśli w trakcie projektu pojawi się zadanie z innej domeny — czytaj odpowiedni skill zanim napiszesz pierwszą linijkę.

### Weryfikacja API

Nie wymyślaj sygnatur funkcji, nazw pól JSON ani endpointów. Jeśli nie masz stuprocentowej pewności — sprawdź dokumentację (docs.rs, oficjalne API docs, źródła crate'a). To dotyczy szczególnie: libcdio, MusicBrainz WS/2, Cover Art Archive, LRCLIB i API Tauri 2.

---

## Licencja

Projekt jest na **GPL-3.0-or-later**. To nie jest wybór estetyczny — linkujemy `libcdio-paranoia`, które jest na GPL, więc cała aplikacja musi być GPL. Świadomie rezygnujemy z architektury sidecar właśnie po to, żeby móc linkować bezpośrednio.

- Dodaj plik `LICENSE` z pełnym tekstem GPL-3.0.
- Nagłówki licencyjne w plikach źródłowych: wystarczy `SPDX-License-Identifier: GPL-3.0-or-later` w pierwszej linii każdego pliku `.rs`.
- W README wypisz zależności wraz z ich licencjami.

---

## Stack

**Frontend:** Nuxt (najnowsza stabilna wersja) w trybie SPA (`ssr: false`), TypeScript, pnpm, **Tailwind jako jedyna warstwa CSS**.
**Backend:** Rust, Tauri 2.
**Baza:** SQLite na lokalną bibliotekę i cache metadanych.

### Kluczowe zależności Rust (zweryfikuj wersje przed dodaniem)

| Cel | Rozwiązanie |
| --- | --- |
| Odczyt audio z CD | `libcdio` + `libcdio-paranoia` przez FFI (`bindgen` w `build.rs`) |
| TOC / dostęp do napędu | `libcdio` jako wspólna baza dla wszystkich platform; natywne uzupełnienia tam, gdzie nie wystarcza: crate `windows` (`DeviceIoControl`, `IOCTL_STORAGE_CHECK_VERIFY`, `IOCTL_STORAGE_EJECT_MEDIA`), `nix`/`libc` na Linuxie (`CDROMEJECT`, `SG_IO`), IOKit na macOS |
| Ścieżki systemowe | `directories` albo API ścieżek Tauri — nigdy ręcznie |
| MusicBrainz | `musicbrainz_rs` (jeśli nie obsługuje encji `discid` — uderz w `/ws/2/discid/{id}` bezpośrednio przez `reqwest`) |
| HTTP | `reqwest` + `serde` / `serde_json` |
| Tagowanie plików | `lofty` |
| Enkodowanie | FFmpeg przez `ffmpeg-next` albo `rsmpeg` — jedna zależność zamiast ośmiu osobnych bibliotek |
| Enkodowanie APE | Monkey's Audio SDK, za opcjonalną feature flagą `ape` |
| Baza | `sqlx` z SQLite albo `tauri-plugin-sql` |
| Logowanie | `tracing` + `tracing-subscriber` |
| Błędy | `thiserror` w warstwie bibliotecznej, `anyhow` na granicy komend Tauri |

### Build

Zależności natywne (`libcdio`, `libcdio-paranoia`, FFmpeg) pochodzą z menedżera pakietów danego systemu:

- **Windows** — MSYS2 (mingw-w64) albo vcpkg
- **Linux** — pacman / apt / dnf, zależnie od dystrybucji
- **macOS** — Homebrew

W `docs/BUILD.md` opisz kompletne, powtarzalne kroki **osobno dla każdej platformy**: nazwy pakietów, jak wskazać `bindgen` ścieżki do nagłówków, jak podpiąć biblioteki i jak dołączyć je do bundla Tauri. Na macOS uwzględnij `@rpath` i `install_name_tool`. Docelowe formaty bundla: NSIS/MSI, AppImage + deb, `.dmg`.

Notaryzacja na macOS i podpisywanie na Windows to temat na później — na razie tylko odnotuj w BUILD.md, czego będzie wymagało.

---

## Architektura

Rdzeń logiki trzymaj w osobnym crate `core/`, niezależnym od Tauri — komendy Tauri mają być cienką warstwą nad nim. Dzięki temu da się testować rip i metadane bez odpalania GUI.

```
src-tauri/          # warstwa Tauri: komendy, stan aplikacji, bundling
crates/cdio-sys/    # bindingi FFI do libcdio / libcdio-paranoia
crates/core/        # TOC, discid, rip, weryfikacja, metadane, tagowanie
app/                # Nuxt
```

### Moduły `core`

1. **`drive`** — trait `Drive` i implementacje per system: wykrywanie napędów, odczyt TOC, wykrycie włożonej płyty, wysuwanie. `libcdio` jako wspólna baza, natywny kod tylko tam, gdzie nie wystarcza. To **jedyne** miejsce w projekcie, w którym wolno użyć `#[cfg(target_os)]`.
2. **`toc`** — surowy TOC (offsety ścieżek, lead-out, pre-emphasis, flagi data track).
3. **`discid`** — MusicBrainz Disc ID liczony samodzielnie z TOC (SHA-1 po sformatowanych offsetach, base64 z podmienionym alfabetem `+/=` → `._-`). Dodatkowo FreeDB ID dla kompatybilności. **Napisz do tego testy jednostkowe na znanych, zahardkodowanych TOC-ach** — to jest miejsce, gdzie błąd o jeden sektor psuje wszystko dalej i jest trudny do wyśledzenia.
4. **`rip`** — ekstrakcja audio przez paranoia, kompensacja offsetu napędu (w samplach, konwencja EAC), obsługa pregapów, retry na błędach, raportowanie statusów paranoi.
5. **`verify`** — CRC32 w stylu EAC oraz AccurateRip V1/V2 liczone lokalnie. **Uwaga na porównanie z bazą online:** dostęp do bazy AccurateRip należy do Illustrate i wymaga zgody na użycie w cudzym oprogramowaniu. Sam algorytm jest jawny, baza nie. Dlatego jako źródło weryfikacji online implementuj **CTDB (CUETools DB)**, które ma otwarte API. Sumy AccurateRip licz i pokazuj lokalnie, ale nie odpytuj ich bazy.
6. **`encode`** — trait `Encoder` i rejestr formatów. Mapowanie: FLAC → flac/FLAC, WAV → PCM s16le/WAV, AIFF → PCM s16be/AIFF, ALAC → alac/M4A, M4A (AAC) → natywny enkoder aac/MP4, MP3 → libmp3lame, AAC → natywny aac/ADTS, Ogg Vorbis → libvorbis/Ogg, APE → Monkey's Audio SDK. Enkodowanie do wszystkich wybranych formatów równolegle, z jednego odczytu płyty — powtórny odczyt to błąd projektowy. Ustawienia jakości per format w konfiguracji, nie w kodzie. Dodanie kolejnego formatu ma wymagać jednej implementacji traitu i zera zmian w module `rip`.
7. **`metadata`** — **nie jedno źródło, tylko kaskada.** Każde za wspólnym traitem `MetadataSource`, próbowane po kolei, dopiero gdy poprzednie nic nie zwróciło. Awaria lub timeout jednego źródła nigdy nie przerywa całości.

   1. **CD-TEXT z płyty** (libcdio) — bez internetu, rzadkie, ale autorytatywne
   2. **MusicBrainz po Disc ID** — `inc=recordings+artist-credits+release-groups+labels`
   3. **CTDB** — replikuje MusicBrainz, Discogs i freeDB, obsługuje wyszukiwanie rozmyte po CDTOC, więc trafia tam, gdzie sam Disc ID nie trafił
   4. **GnuDB** — następca freedb: `gnudb.gnudb.org`, protokół CDDB1, port 8880 albo HTTP na `/~cddb/cddb.cgi`
   5. **MCN/UPC ze subkanału płyty → Discogs po barcode** — fizyczna płyta często niesie kod kreskowy, a Discogs ma najlepszą bazę wydań fizycznych
   6. **ISRC per ścieżka → MusicBrainz po ISRC** — ratuje sytuację, gdy wydanie jest nieznane, ale nagrania już tak

   Gdy jeden Disc ID mapuje się na kilka wydań albo źródła się różnią — **nie zgaduj i nie scalaj po cichu**. Pokaż warianty obok siebie z etykietą pochodzenia i pozwól wybrać. Z `medium.position` i `medium-count` wyciągnij `discnumber`/`totaldiscs`.

   Uzupełniająco: okładka z Cover Art Archive → obrazy Discogs → iTunes Search API (bez klucza) → własny plik użytkownika. Gatunek z Discogs (`genre` + `style`) i Last.fm (tagi), bo MusicBrainz jest tu ubogi.

   Klucze API w ustawieniach, nigdy w repo. Aplikacja ma być w pełni użyteczna bez żadnego z nich. Respektuj limity zapytań i wymagany User-Agent każdego serwisu.

8. **`manual`** — ręczne wyszukiwanie płyty. To nie jest funkcja awaryjna doklejona na końcu, tylko normalna, zawsze dostępna ścieżka. Pole wyszukiwania po tytule i wykonawcy przeszukujące MusicBrainz i Discogs jednocześnie; wyniki z liczbą ścieżek i czasem trwania, z podświetleniem tych, gdzie liczba ścieżek zgadza się z TOC w napędzie; możliwość wklejenia linku albo identyfikatora wydania; pełna ręczna edycja wszystkich pól łącznie z tytułami ścieżek. Ręcznie wprowadzone dane zapisuj lokalnie i wiąż z Disc ID, żeby ta sama płyta włożona ponownie została rozpoznana od razu. Wysyłkę uzupełnionych danych do MusicBrainz proponuj, ale wykonuj **wyłącznie** po wyraźnym potwierdzeniu.
9. **`lyrics`** — LRCLIB. `GET https://lrclib.net/api/get` z `track_name`, `artist_name`, `album_name`, `duration` (sekundy, z TOC). Odpowiedź zawiera m.in. `plainLyrics`, `syncedLyrics`, `instrumental`. Fallback na `/api/search`. Ustaw własny User-Agent. Zapis: `LYRICS` w Vorbis comment dla FLAC, `USLT` dla MP3, a wersję zsynchronizowaną dodatkowo jako plik `.lrc` obok utworu.
10. **`tag`** — zapis wszystkiego przez `lofty`, osadzanie okładki, generowanie CUE sheet i logu ripu.

### Komunikacja z frontendem

Progress ripu leci przez `tauri::ipc::Channel<T>` (nie przez globalne eventy — jeden kanał na jedną operację). Zdefiniuj wspólne typy zdarzeń i wygeneruj z nich typy TS, żeby front nie duplikował definicji ręcznie.

---

## Etapy

Realizuj po kolei. Po każdym etapie zatrzymaj się i pokaż, co działa.

**Etap 1 — szkielet.** Repo, `.gitignore`, LICENSE, Nuxt + Tauri 2 wstają i się budują, pnpm skonfigurowany, Tailwind podpięty, warstwa i18n z plikami `en` i `pl` na miejscu, workspace Cargo z podziałem na crate'y. Commit.

**Etap 2 — napęd i TOC.** Trait `Drive` z implementacjami dla wszystkich trzech systemów, odczyt TOC, wyliczanie Disc ID z testami. W UI: wykryta płyta i lista ścieżek z czasami. Zanim przejdziesz dalej, potwierdź, że kompilacja przechodzi na każdej platformie. Commit.

**Etap 3 — metadane.** Najpierw MusicBrainz po Disc ID i ekran wyboru wydania, gdy jest ich kilka — commit. Potem kolejne źródła kaskady, każde osobnym commitem. Na koniec ręczne wyszukiwanie i pełna edycja pól. Okładka z CAA. Commit.

**Etap 4 — rip.** FFI do paranoi, ekstrakcja do WAV, offset napędu, progress przez kanał, anulowanie w trakcie. Commit.

**Etap 5 — enkodowanie i tagi.** Najpierw sam FLAC z pełnym tagowaniem przez lofty, okładką, szablonami nazw i CUE + log — commit. Dopiero potem trait `Encoder` i reszta formatów, każdy osobnym commitem, z enkodowaniem równoległym. APE na końcu, za feature flagą.

**Etap 6 — weryfikacja.** CRC32 EAC, sumy AccurateRip liczone lokalnie, sprawdzenie w CTDB, czytelny wynik w UI. Commit.

**Etap 7 — teksty.** LRCLIB, dopasowanie po czasie trwania, zapis do tagów i `.lrc`, ręczna korekta gdy nie trafi. Commit.

**Etap 8 — biblioteka.** SQLite, historia ripów, ustawienia (ścieżki, offset per napęd, tokeny API, szablony nazw). Commit.

---

## Konwencje kodu

- Rust: `cargo fmt` i `cargo clippy -- -D warnings` muszą przechodzić przed każdym commitem.
- Frontend: ESLint + Prettier, komponenty w `<script setup lang="ts">`, Composition API.
- **CSS wyłącznie przez Tailwind.** Żadnych `<style scoped>`, plików `.css` per komponent ani bibliotek komponentów niosących własne style. Wartości arbitralne (`w-[327px]`) tylko gdy skala Tailwinda naprawdę nie ma odpowiednika. Powtarzające się układy klas wydzielaj do komponentów, nie do `@apply`. Przy instalacji sprawdź aktualną wersję i oficjalną metodę integracji z Nuxtem — od v4 jest to plugin Vite `@tailwindcss/vite`, a motyw konfiguruje się przez `@theme` w CSS zamiast `tailwind.config.js`; nie sugeruj się poradnikami do v3.
- Nazwy po angielsku, wszystkie w kodzie. Bez wyjątków.

### Wielojęzyczność

Aplikacja jest wielojęzyczna **od pierwszego commita**. Nie ma etapu „najpierw po polsku, i18n dorobimy potem" — dokładanie tłumaczeń do gotowego UI oznacza przeczesywanie każdego komponentu ręcznie.

- **Angielski jest językiem źródłowym**, polski pierwszym tłumaczeniem. Klucze i wartości domyślne po angielsku — projekt jest open source i tłumaczenia będą przychodzić od ludzi, którzy polskiego nie znają.
- Dodanie kolejnego języka = wrzucenie jednego pliku do katalogu z tłumaczeniami. Zero zmian w kodzie.
- **Żadnego stringa wklejonego do komponentu.** Każdy tekst widoczny dla użytkownika idzie przez i18n — etykiety, tooltipy, placeholdery, tytuły okien, komunikaty błędów.
- Użyj oficjalnego modułu i18n do Nuxta. Sprawdź aktualną wersję i metodę konfiguracji, nie sugeruj się starszymi poradnikami.
- Język wykrywaj z ustawień systemu przez API Tauri, z ręczną zmianą w ustawieniach. Fallback na angielski przy brakującym tłumaczeniu — nigdy goły klucz na ekranie.
- Klucze hierarchiczne i opisowe: `rip.progress.verifying`, `metadata.source.musicbrainz`, `error.drive.permissionDenied`. Nie `label1`, nie całe zdania jako klucze.
- Liczby, daty i czasy trwania formatuj przez `Intl`. Separator dziesiętny i format daty różnią się między lokalizacjami.
- Teksty z liczebnikami zawsze przez formy mnogie i18n — polski ma trzy formy (`1 ścieżka`, `2 ścieżki`, `5 ścieżek`), angielski dwie. Sklejanie liczby ze stringiem to błąd.

**Warstwa Rust nie tłumaczy niczego.** Backend zwraca kody błędów i parametry, nigdy gotowe zdania:

```rust
DriveError::PermissionDenied { device: "/dev/sr0", group: "cdrom" }
```

Front mapuje to na `error.drive.permissionDenied` i podstawia parametry. Jeśli backend zacznie zwracać teksty do wyświetlenia, cała warstwa tłumaczeń przestaje działać — a wyjdzie to dopiero przy drugim języku.

**Nie tłumaczymy:** nazw formatów i kodeków, tagów, nazw pól metadanych, treści z baz zewnętrznych, szablonów nazw plików ani logów. Log ripu jest artefaktem diagnostycznym i zostaje po angielsku.
- Testy jednostkowe obowiązkowo dla: Disc ID, parsowania TOC, CRC AccurateRip, szablonów nazw plików. Reszta wedle uznania.
- Błędy pokazywane użytkownikowi mają być konkretne. „Nie udało się odczytać płyty" jest bezużyteczne — napisz, która ścieżka, na którym sektorze i co zwrócił napęd.

---

**macOS montuje płyty audio automatycznie.** System podpina CD jako wolumin z plikami `.aiff` i blokuje surowy odczyt urządzenia. Przed ripem trzeba odmontować dysk (`diskutil unmountDisk`), a po zakończeniu przywrócić stan. To najczęstsza przyczyna błędu „nie mogę otworzyć urządzenia" na macu. Dodatkowo Apple Silicon nie ma wbudowanych napędów, więc jedyny scenariusz testowy to zewnętrzny napęd USB.

**Linux: uprawnienia do `/dev/sr0`.** Zwykle wymagana przynależność do grupy `cdrom` albo `optical`. Komunikat błędu ma to mówić wprost i podawać nazwę grupy, a nie zwracać gołe „odmowa dostępu".

**Sanityzacja nazw plików.** Linux i macOS dopuszczają znaki zakazane na Windows. Domyślnie stosuj wariant najbardziej restrykcyjny na wszystkich platformach — biblioteka muzyczna często ląduje na NAS-ie albo dysku współdzielonym. Luźniejszy tryb tylko jako świadomy wybór w ustawieniach.

**Offset napędu to cecha modelu urządzenia, nie systemu.** Zapisuj go po identyfikatorze producent/model, żeby ten sam napęd nie wymagał ponownej kalibracji po przełączeniu systemu.

**Źródła metadanych bywają sprzeczne.** Ten sam Disc ID potrafi zwrócić różne tytuły z MusicBrainz, CTDB i GnuDB — zwłaszcza przy reedycjach i wydaniach regionalnych. Nigdy nie scalaj tego po cichu ani nie ustalaj „większością głosów". Pokaż warianty z etykietą pochodzenia i pozwól wybrać.

**Timeout jednego serwisu nie może blokować ripu.** Kaskada ma lecieć dalej, a użytkownik musi móc zacząć ripowanie z niepełnymi metadanymi i uzupełnić je później.

## Czego nie robić

- Nie używaj architektury sidecar ani wywoływania zewnętrznych binarek do ripowania. Cała logika ma być w naszym procesie.
- Nie buduj FFmpeg z `--enable-nonfree` ani z `fdk-aac` — binarka staje się nieredystrybuowalna i jest to nie do pogodzenia z GPL. Do AAC używaj natywnego enkodera FFmpeg. Build LGPL z `libmp3lame` i `libvorbis` w zupełności wystarczy.
- Nie pisz własnych enkoderów dla formatów, które FFmpeg już obsługuje.
- Nie traktuj „M4A" jako kodeka — to kontener. W UI muszą być dwie osobne pozycje: `M4A (AAC)` i `M4A (ALAC)`.
- Nie odpytuj bazy AccurateRip (patrz wyżej).
- Nie ustawiaj `unsafe` poza warstwą `cdio-sys` i modułem `drive`. Cała reszta ma być bezpieczna.
- Nie wklejaj tekstów widocznych dla użytkownika bezpośrednio do komponentów ani do kodu Rusta — wszystko przez i18n.
- Nie pisz CSS poza Tailwindem — żadnych `<style scoped>` ani plików `.css` per komponent.
- Nie wstawiaj `#[cfg(target_os)]` poza modułem `drive`. Jeśli zaczyna ci się to gdzieś zdawać potrzebne, to znak, że abstrakcja jest w złym miejscu.
- Nie opieraj identyfikacji płyty na jednym źródle i nie scalaj sprzecznych wyników po cichu.
- Nie wysyłaj niczego do MusicBrainz bez wyraźnego potwierdzenia użytkownika.
- Nie wymagaj żadnego klucza API do działania aplikacji.
- Nie hardkoduj ścieżek ani separatorów. Nigdzie.
- Nie dodawaj zależności, która nie buduje się na wszystkich trzech systemach.
- Nie pushuj.

---

## Na start

Zacznij od potwierdzenia planu etapu 1 i wypisania konkretnych wersji pakietów, które zamierzasz zainstalować (po ich sprawdzeniu, nie z pamięci). Potem działaj.
