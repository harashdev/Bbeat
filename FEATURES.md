# BBeat – Full Feature List

## Listening

- Search and stream music from YouTube / YouTube Music.
- Home page with *Jump back in*, *Recommended for today* (refreshed daily from your listening taste), *Quick picks*, *Collections for you*, *My supermix* and *Favorites fading into oblivion*.
- **Explore** and **Trending** pages.
- Browse by **17 moods**: Relax, Sad, Romantic, Party, Sleep and more.
- **Best of artists** shelf: artists are ranked from chart position, views and likes, with a **"This Is"** collection for each (up to 50 signature tracks).
- Follow artists, see *Most played*, and a *Your favorite artists* shelf.
- Queue, shuffle, repeat playlist / repeat one, seek bar and volume control.
- **Crossfade** between tracks (beta).
- Optional **music video playback** in place of cover art.
- Explicit tracks can be hidden and skipped (off by default).
- Remembers your playback state, including shuffle and repeat modes.

## Lyrics and track info

- Time-synced **karaoke lyrics** from SimpMusic Lyrics, with LRCLIB as a fallback. Tap a line to jump to it.
- **Now Playing** panel with three tabs: Now Playing, Lyrics and Up next.
- **SponsorBlock** is always on and skips sponsor, intro and outro segments.
- **Return YouTube Dislike** shows like and dislike counts.

## Sound

- **10-band equalizer** with a pre-amp.
- **Echo** effect with delay time, feedback and mix.
- **Reverb** effect with room, hall and metal plate presets.
- With effects on, audio plays through the server stream (not the embedded YouTube player), and falls back to the YouTube player if the stream is unavailable.

## Friends and listening together

- **Shared listening rooms** in real time, with playback kept in sync.
- Join with a **room code** or a **room link**, and invite friends into your room.
- **Room chat**.
- Add friends, send and receive friend requests, accept or decline.
- **Blend**: mix your taste with a friend's into a single playlist.
- View your friends' profiles.

## Profile and appearance

- Avatar and Discord-style banner supporting **PNG, JPG, WEBP and animated GIF** (up to 10 MB).
- **Image cropper**: drag, scroll to zoom, X / Y / Zoom sliders, and zoom out to see the whole image. GIFs stay animated after cropping.
- Banner color, status line in a bubble next to your avatar, and a profile card.
- **Profile background**: pick one or upload your own image or GIF, with a live preview and a *Preview the whole app* button.
- Accent color follows the theme you choose.
- Interface in **English and Vietnamese**.

## Your library

- Register, sign in, change your display name or password (other devices are signed out), delete your account.
- **Favorites**, **listening history** (tracking can be turned off, history can be cleared) and **playlists** (create, delete, add and remove tracks).
- **Cross-device sync** of your data when signed in.
- **Playlist import** from YouTube (public or unlisted), Spotify, Apple Music and SoundCloud: up to 300 tracks per import, in the original order, with a report of skipped tracks. Playlists can also be imported from a JSON file.
- **Backup and restore** of all your data.
- **Offline downloads** for songs.
- **Offline playlists**: open the app and browse your playlists without a connection.

## Integrations and system

- **Discord sign-in** (OAuth) and **Discord Rich Presence** ("Listening to…" on your Discord profile).
- **Proxy** support for search and audio streams.
- Multiple fallbacks when YouTube blocks an audio stream: ANDROID_VR, IOS, embedded TV client, yt-dlp (with cookie support) and Piped.
- Automatic **update check** through GitHub Releases, with a manual check button.
- *About* screen with version, author, a support link and the list of third-party libraries.
- **Admin** section (admins only): technical error notifications and app icon change.

## Android

- Background playback with a foreground service and MediaSession.
- Notification with Previous / Play-Pause / Next buttons and cover art, plus lock screen and headset controls.
- Pauses automatically when headphones are unplugged or a call comes in, and resumes after the call.
- Holds a WakeLock and WifiLock while playing, and asks to be excluded from battery optimization.
- Saves downloaded music to `Music/BBeat`.
- Share songs with the Android share sheet.
- Opens a saved copy of the interface when the server can't be reached.
- Phone layout, plus an optional *Desktop layout* that matches the PC interface, and performance tweaks (no blur, lazy-loaded images).
- Reloads automatically if the system reclaims the WebView process.

## Windows

- Desktop app built with Tauri v2.
- Discord Rich Presence through Discord IPC or the Discord Social SDK.
- Experimental **pure Rust client (egui)**: no WebView2, so it is light on RAM, with native AAC/MP4 playback. It currently has sign-in, home, search, favorites, history, queue, playlists and the player bar. Listening rooms, friends, karaoke lyrics and settings are not in it yet.

## Web

- Runs in any modern browser and can be installed as an app, with a manifest and service worker so the interface opens offline.
