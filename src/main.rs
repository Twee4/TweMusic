use std::{fs, path::PathBuf, sync::mpsc::{self, Receiver, Sender, TryRecvError}, thread, time::{Duration, Instant}};

use eframe::egui::{
    self, pos2, vec2, Align, CentralPanel, Context, Frame, Layout, Margin, Rect, Rounding, ScrollArea,
    Sense, SidePanel, ThemePreference, TopBottomPanel, UiBuilder,
};
use souvlaki::{MediaControlEvent, MediaPosition};
use twe_music::{
    icons::Icon,
    import,
    library::{self, Library, SavedTrack},
    m3::{self, ButtonKind, CardKind, IconButtonKind, SeekEvent},
    media::Media,
    notifications,
    models::{Screen, Track, TrackId},
    mpv::{self, MpvError, MpvPlayer},
    settings::{self, Settings},
    state::{AppState, PlaybackStatus},
    theme::{self, shape, ColorScheme, Role, WindowSize},
    tray::{TrayCommand, TrayHandle},
    ytmusic,
};

type ImportResult = Result<(String, Vec<Track>), String>;

/// How long a snackbar message stays on screen.
const SNACKBAR_TIME: Duration = Duration::from_secs(4);

fn format_seconds(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

/// Paints album art into `rect`, centre-cropped to fill it, once the image has loaded.
/// While it loads (or if it fails, e.g. offline) nothing is drawn, so the placeholder
/// underneath stays visible and egui's warning glyph never appears.
fn paint_album_art(ui: &egui::Ui, url: &str, rect: Rect) {
    let image = egui::Image::from_uri(url).show_loading_spinner(false);
    let Ok(egui::load::TexturePoll::Ready { texture }) = image.load_for_size(ui.ctx(), rect.size()) else {
        return;
    };
    let aspect = if texture.size.y > 0.0 { texture.size.x / texture.size.y } else { 1.0 };
    let (inset_x, inset_y) = if aspect > 1.0 {
        ((1.0 - 1.0 / aspect) / 2.0, 0.0)
    } else if aspect > 0.0 {
        (0.0, (1.0 - aspect) / 2.0)
    } else {
        (0.0, 0.0)
    };
    image
        .uv(Rect::from_min_max(pos2(inset_x, inset_y), pos2(1.0 - inset_x, 1.0 - inset_y)))
        .rounding(Rounding::same(shape::SMALL))
        .paint_at(ui, rect);
}

/// What the user asked for in a track row; applied after the row is drawn.
enum RowAction {
    None,
    Play,
    ToggleLike,
    AddTo(usize),
    Remove,
    OpenWeb,
}

struct TweMusicApp {
    state: AppState,
    search_rx: Option<Receiver<Result<Vec<Track>, String>>>,
    search_in_flight: bool,
    mpv: Option<MpvPlayer>,
    local_file_path: String,
    /// Transient playback status shown in the player bar ("Loading…", "Paused").
    player_message: Option<String>,
    /// Feedback for user actions and errors, shown as a snackbar.
    snackbar: Option<(String, Instant)>,
    ytdlp_available: Option<bool>,
    last_poll: Instant,
    position: Option<f64>,
    duration: Option<f64>,
    played_something: bool,
    loading_since: Option<Instant>,
    library: Library,
    library_path: Option<PathBuf>,
    selected_playlist: usize,
    new_playlist_name: String,
    import_url: String,
    import_rx: Option<Receiver<ImportResult>>,
    media: Option<Media>,
    media_rx: Receiver<MediaControlEvent>,
    media_synced: Option<(Option<TrackId>, PlaybackStatus)>,
    seek_drag: Option<f64>,
    theme_pref: ThemePreference,
    settings: Settings,
    settings_path: Option<PathBuf>,
    tray: Option<TrayHandle>,
    tray_tx: Sender<TrayCommand>,
    tray_rx: Receiver<TrayCommand>,
    tray_synced: Option<(Option<TrackId>, PlaybackStatus)>,
    /// Set when the user really wants to quit (tray menu), so closing doesn't hide to the tray.
    quit_requested: bool,
    notice_tx: Sender<String>,
    notice_rx: Receiver<String>,
}

impl TweMusicApp {
    fn new(ctx: Context) -> Self {
        theme::apply(&ctx);
        let theme_pref = theme::load_preference();
        ctx.set_theme(theme_pref);
        egui_extras::install_image_loaders(&ctx);

        let (media_tx, media_rx) = mpsc::channel();
        let repaint_ctx = ctx.clone();
        let media = Media::new(media_tx, move || repaint_ctx.request_repaint());
        let library_path = library::default_path();
        let library = library_path.as_deref().map(Library::load_from).unwrap_or_else(Library::new);
        let settings_path = settings::default_path();
        let settings = settings_path.as_deref().map(Settings::load_from).unwrap_or_default();
        let (tray_tx, tray_rx) = mpsc::channel();
        let (notice_tx, notice_rx) = mpsc::channel();
        let mut app = Self {
            state: AppState::new(), search_rx: None, search_in_flight: false,
            mpv: None, local_file_path: String::new(), player_message: None, snackbar: None,
            ytdlp_available: None, last_poll: Instant::now(), position: None, duration: None,
            played_something: false, loading_since: None,
            library, library_path, selected_playlist: 0, new_playlist_name: String::new(),
            import_url: String::new(), import_rx: None,
            media, media_rx, media_synced: None, seek_drag: None,
            theme_pref,
            settings, settings_path, tray: None, tray_tx, tray_rx, tray_synced: None, quit_requested: false,
            notice_tx, notice_rx,
        };
        if app.settings.tray {
            app.start_tray(&ctx);
        }
        app
    }

    fn save_settings(&mut self) {
        if let Some(path) = &self.settings_path {
            if let Err(error) = self.settings.save_to(path) {
                self.notify(format!("Could not save settings: {error}"));
            }
        }
    }

    /// Starts the tray icon. Returns false (and says why) if the desktop has no tray.
    fn start_tray(&mut self, ctx: &Context) -> bool {
        if self.tray.is_some() { return true; }
        let wake_ctx = ctx.clone();
        match TrayHandle::spawn(self.tray_tx.clone(), move || wake_ctx.request_repaint()) {
            Ok(handle) => {
                self.tray = Some(handle);
                self.tray_synced = None;
                true
            }
            Err(message) => {
                self.notify(message);
                false
            }
        }
    }

    fn stop_tray(&mut self) {
        if let Some(handle) = self.tray.take() {
            handle.shutdown();
        }
    }

    /// Really quits (stops playback and exits), even when the tray keeps the app alive on close.
    fn quit(&mut self, ctx: &Context) {
        self.quit_requested = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn show_window(ctx: &Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    /// Handles clicks on the tray icon and its menu.
    fn poll_tray(&mut self, ctx: &Context) {
        while let Ok(message) = self.notice_rx.try_recv() {
            self.notify(message);
        }
        while let Ok(command) = self.tray_rx.try_recv() {
            match command {
                TrayCommand::Show => Self::show_window(ctx),
                TrayCommand::PlayPause => self.toggle_local_playback(),
                TrayCommand::Next => self.skip(true),
                TrayCommand::Previous => self.skip(false),
                TrayCommand::Quit => self.quit(ctx),
            }
        }
    }

    /// Keeps the tray's play/pause label and tooltip in step with playback.
    fn sync_tray(&mut self) {
        let Some(tray) = self.tray.as_ref() else { return; };
        let key = (self.state.current_track, self.state.playback);
        if self.tray_synced == Some(key) { return; }
        self.tray_synced = Some(key);
        let name = self
            .state
            .current_track()
            .map(|track| format!("{} - {}", track.title, track.artist))
            .unwrap_or_default();
        tray.update(self.state.playback == PlaybackStatus::Playing, &name);
    }

    /// With the tray on, closing the window minimises it so the music keeps playing.
    /// (Minimised rather than hidden: that works the same on X11 and Wayland.)
    fn handle_close(&mut self, ctx: &Context) {
        if self.tray.is_some() && !self.quit_requested && ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
    }

    /// Shows a snackbar message.
    fn notify(&mut self, message: impl Into<String>) {
        self.snackbar = Some((message.into(), Instant::now()));
    }

    fn seek_to(&mut self, seconds: f64) {
        let Some(player) = self.mpv.as_mut() else { return; };
        match player.seek_to(seconds) {
            Ok(()) => self.position = Some(seconds),
            Err(error) => self.notify(error.to_string()),
        }
    }

    /// Handles commands from media keys and desktop media widgets.
    fn poll_media(&mut self) {
        while let Ok(event) = self.media_rx.try_recv() {
            match event {
                MediaControlEvent::Play => {
                    if self.state.playback == PlaybackStatus::Paused { self.toggle_local_playback(); }
                }
                MediaControlEvent::Pause | MediaControlEvent::Stop => {
                    if self.state.playback == PlaybackStatus::Playing { self.toggle_local_playback(); }
                }
                MediaControlEvent::Toggle => self.toggle_local_playback(),
                MediaControlEvent::Next => self.skip(true),
                MediaControlEvent::Previous => self.skip(false),
                MediaControlEvent::SetPosition(MediaPosition(position)) => self.seek_to(position.as_secs_f64()),
                _ => {}
            }
        }
    }

    /// Tells the desktop about the current track and state, only when they change.
    fn sync_media(&mut self) {
        let Some(media) = self.media.as_mut() else { return; };
        let key = (self.state.current_track, self.state.playback);
        if self.media_synced == Some(key) { return; }
        self.media_synced = Some(key);
        media.update(self.state.current_track(), self.state.playback, self.position);
    }

    fn save_library(&mut self) {
        if let Some(path) = &self.library_path {
            if let Err(error) = self.library.save_to(path) {
                self.notify(format!("Could not save your library: {error}"));
            }
        }
    }

    fn start_import(&mut self, ctx: &Context) {
        if self.import_rx.is_some() { return; }
        let url = self.import_url.trim().to_owned();
        if !import::is_supported_playlist_url(&url) {
            self.notify("Paste a public playlist link from music.youtube.com or youtube.com (it must contain list=).".to_owned());
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.import_rx = Some(rx);
        self.notify("Importing playlist…".to_owned());
        let repaint = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(import::import_playlist(&url));
            repaint.request_repaint();
        });
    }

    fn poll_import(&mut self) {
        let Some(receiver) = &self.import_rx else { return; };
        let outcome = match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("Import stopped unexpectedly.".to_owned()),
        };
        self.import_rx = None;
        match outcome {
            Ok((name, tracks)) => {
                let (index, added) = self.library.import(&name, &tracks);
                self.selected_playlist = index;
                self.import_url.clear();
                self.save_library();
                let saved_name = self.library.playlists[index].name.clone();
                self.notify(format!("Imported {added} tracks into \"{saved_name}\"."));
            }
            Err(error) => self.notify(error),
        }
    }

    fn play_playlist(&mut self, index: usize) {
        let tracks: Vec<Track> = self.library.playlists.get(index)
            .map(|playlist| playlist.tracks.iter().map(SavedTrack::to_track).collect())
            .unwrap_or_default();
        let Some(first) = self.state.set_queue(tracks) else {
            self.notify("That playlist is empty.".to_owned());
            return;
        };
        if let Some(track) = self.state.tracks.iter().find(|track| track.id == first).cloned() {
            self.play_remote(&track);
        }
    }

    fn ensure_player(&mut self) -> Result<&mut MpvPlayer, MpvError> {
        if self.mpv.is_none() {
            self.mpv = Some(MpvPlayer::start()?);
        }
        Ok(self.mpv.as_mut().expect("player was just started"))
    }

    /// Streams a YouTube Music track's audio through MPV (which resolves it with yt-dlp).
    fn play_remote(&mut self, track: &Track) {
        let Some(url) = track.video_id.as_deref().and_then(ytmusic::youtube_music_url) else {
            self.notify("YouTube returned an invalid video ID.".to_owned());
            return;
        };
        if !*self.ytdlp_available.get_or_insert_with(mpv::ytdlp_available) {
            self.ytdlp_available = None; // check again next time, in case it was just installed
            self.notify("yt-dlp is required to play YouTube tracks. Install it (for example `pipx install yt-dlp`) and try again.".to_owned());
            return;
        }
        let result = self.ensure_player().and_then(|player| player.load_url(&url));
        match result {
            Ok(()) => {
                if !self.state.tracks.iter().any(|existing| existing.id == track.id) {
                    self.state.tracks.push(track.clone());
                }
                self.state.play_track(track.id);
                self.position = None;
                self.duration = None;
                self.played_something = false;
                self.loading_since = Some(Instant::now());
                self.player_message = Some(format!("Loading {}…", track.title));
            }
            Err(error) => self.notify(error.to_string()),
        }
    }

    fn skip(&mut self, forward: bool) {
        let Some(id) = self.state.adjacent_remote(forward) else { return; };
        if let Some(track) = self.state.tracks.iter().find(|track| track.id == id).cloned() {
            self.play_remote(&track);
        }
    }

    /// Polls MPV for progress, detects the end of a track, and auto-advances the queue.
    fn tick_player(&mut self, ctx: &Context) {
        if self.state.playback != PlaybackStatus::Playing { return; }
        let interval = Duration::from_millis(500);
        ctx.request_repaint_after(interval);
        if self.last_poll.elapsed() < interval { return; }
        let Some(player) = self.mpv.as_mut() else { return; };
        self.last_poll = Instant::now();
        let position = player.position();
        let duration = player.duration();
        let idle = player.is_idle();

        self.position = position;
        self.duration = duration;
        if position.is_some_and(|value| value > 0.0) {
            if !self.played_something {
                self.player_message = None;
                let in_background = ctx.input(|input| input.viewport().focused) == Some(false);
                if self.settings.notifications && in_background {
                    if let Some(track) = self.state.current_track() {
                        notifications::show_track(track);
                    }
                }
            }
            self.played_something = true;
            self.loading_since = None;
        }
        if !idle { return; }
        if self.played_something {
            self.played_something = false;
            if self.state.adjacent_remote(true).is_some() {
                self.skip(true);
            } else {
                self.state.playback = PlaybackStatus::Stopped;
                self.position = None;
                self.duration = None;
            }
        } else if self.loading_since.is_some_and(|since| since.elapsed() > Duration::from_secs(2)) {
            self.loading_since = None;
            self.state.playback = PlaybackStatus::Stopped;
            self.notify("Could not play this track. Try updating yt-dlp (`yt-dlp -U`).".to_owned());
        }
    }

    fn start_search(&mut self, ctx: &Context) {
        let query = self.state.search_query.trim().to_owned();
        if query.is_empty() || self.search_in_flight { return; }
        let (tx, rx) = mpsc::channel();
        self.search_rx = Some(rx);
        self.search_in_flight = true;
        self.state.search_message = Some("Searching YouTube Music…".to_owned());
        let repaint = ctx.clone();
        thread::spawn(move || {
            let result = ytmusic::search_tracks(&query).map_err(|error| error.to_string());
            let _ = tx.send(result);
            repaint.request_repaint();
        });
    }

    fn poll_search(&mut self) {
        let Some(receiver) = &self.search_rx else { return; };
        match receiver.try_recv() {
            Ok(Ok(tracks)) => {
                let count = tracks.len();
                self.state.add_search_results(tracks);
                self.state.search_message = Some(if count == 0 { "No tracks found." } else { "Search complete." }.to_owned());
                self.search_in_flight = false;
                self.search_rx = None;
            }
            Ok(Err(error)) => {
                self.state.remote_results.clear();
                self.state.search_message = Some(error);
                self.search_in_flight = false;
                self.search_rx = None;
            }
            Err(TryRecvError::Disconnected) => {
                self.state.search_message = Some("Search worker stopped unexpectedly.".to_owned());
                self.search_in_flight = false;
                self.search_rx = None;
            }
            Err(TryRecvError::Empty) => {}
        }
    }

    fn start_local_playback(&mut self) {
        let input = self.local_file_path.trim();
        if input.is_empty() {
            self.notify("Enter a local audio file path first.".to_owned());
            return;
        }
        let path = PathBuf::from(input);
        let canonical = match fs::canonicalize(&path) {
            Ok(path) if path.is_file() => path,
            Ok(_) => { self.notify("The selected path is not a file.".to_owned()); return; }
            Err(error) => { self.notify(format!("Cannot open file: {error}")); return; }
        };
        if let Some(player) = &mut self.mpv {
            match player.load_file(&canonical) {
                Ok(()) => {
                    self.state.playback = PlaybackStatus::Playing;
                    self.player_message = Some(format!("Playing {}", canonical.display()));
                }
                Err(error) => self.notify(error.to_string()),
            }
        } else {
            match MpvPlayer::start().and_then(|mut player| { player.load_file(&canonical)?; Ok(player) }) {
                Ok(player) => {
                    self.mpv = Some(player);
                    self.state.playback = PlaybackStatus::Playing;
                    self.player_message = Some(format!("Playing {}", canonical.display()));
                }
                Err(error) => self.notify(error.to_string()),
            }
        }
    }

    fn toggle_local_playback(&mut self) {
        let Some(player) = &mut self.mpv else {
            self.notify("Choose a local audio file in Settings to start playback.".to_owned());
            return;
        };
        match player.toggle_pause() {
            Ok(paused) => {
                self.state.playback = if paused { PlaybackStatus::Paused } else { PlaybackStatus::Playing };
                self.player_message = Some(if paused { "Paused" } else { "Playing" }.to_owned());
            }
            Err(error) => self.notify(error.to_string()),
        }
    }

    // ---- Material 3 UI -------------------------------------------------------------

    fn page_title(ui: &mut egui::Ui, title: &str, subtitle: &str) {
        let c = theme::colors(ui);
        ui.add_space(8.0);
        ui.label(theme::text(Role::HeadlineMedium, title).color(c.on_surface));
        ui.label(theme::text(Role::BodyLarge, subtitle).color(c.on_surface_variant));
        ui.add_space(16.0);
    }

    fn section_title(ui: &mut egui::Ui, title: &str) {
        let c = theme::colors(ui);
        ui.label(theme::text(Role::TitleLarge, title).color(c.on_surface));
        ui.add_space(4.0);
    }

    /// A Material 3 list item for a track: thumbnail, title and artist, duration, and
    /// play / like / more actions. Clicking the row plays the track.
    fn track_row(&mut self, ui: &mut egui::Ui, track: &Track, index: usize) {
        let c = theme::colors(ui);
        let playable = track.video_id.is_some();
        let narrow = ui.available_width() < 480.0;
        let is_current = self.mpv.is_some() && self.state.current_track == Some(track.id);
        let liked = self.library.is_liked(track);
        let in_library_view = self.state.screen == Screen::Library;
        let playlist_names: Vec<String> = self.library.playlists.iter().map(|playlist| playlist.name.clone()).collect();
        let mut action = RowAction::None;

        let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 72.0), Sense::click());
        let rounding = Rounding::same(shape::MEDIUM);
        let painter = ui.painter().clone();
        if is_current {
            painter.rect_filled(row, rounding, c.secondary_container);
        }
        if response.hovered() {
            painter.rect_filled(row, rounding, theme::with_alpha(c.on_surface, theme::state::HOVER));
        }

        // Leading thumbnail, using the tonal container roles in turn.
        let (thumb_bg, thumb_fg) = if is_current {
            (c.primary, c.on_primary)
        } else {
            match index % 3 {
                0 => (c.primary_container, c.on_primary_container),
                1 => (c.secondary_container, c.on_secondary_container),
                _ => (c.tertiary_container, c.on_tertiary_container),
            }
        };
        let thumb = Rect::from_min_size(pos2(row.left() + 12.0, row.center().y - 24.0), vec2(48.0, 48.0));
        painter.rect_filled(thumb, Rounding::same(shape::SMALL), thumb_bg);
        Icon::Note.paint(&painter, thumb.center(), 24.0, thumb_fg, false);
        if let Some(url) = track.thumbnail_url.as_deref() {
            paint_album_art(ui, url, thumb);
        }

        // Layout: [thumb] [title / artist ...] [duration] [play] [like] [more]
        let actions_width = if playable { 3.0 * 40.0 + 2.0 * 4.0 } else { 0.0 };
        let duration_width = if narrow { 0.0 } else { 52.0 };
        let trailing = Rect::from_min_max(
            pos2(row.right() - 8.0 - actions_width - duration_width, row.top()),
            pos2(row.right() - 8.0, row.bottom()),
        );
        let text_left = thumb.right() + 16.0;
        let text_right = (trailing.left() - 8.0).max(text_left + 40.0);
        let text_rect = Rect::from_min_max(
            pos2(text_left, row.center().y - 22.0),
            pos2(text_right, row.center().y + 22.0),
        );
        let (title_color, subtitle_color) = if is_current {
            (c.on_secondary_container, c.on_secondary_container)
        } else {
            (c.on_surface, c.on_surface_variant)
        };

        ui.allocate_new_ui(
            UiBuilder::new().max_rect(text_rect).layout(Layout::top_down(Align::Min)),
            |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.add(egui::Label::new(theme::text(Role::BodyLarge, &track.title).color(title_color)).truncate().selectable(false));
                ui.add(egui::Label::new(theme::text(Role::BodyMedium, &track.artist).color(subtitle_color)).truncate().selectable(false));
            },
        );

        let popup_id = ui.id().with(("track-menu", track.id.0, index));
        ui.allocate_new_ui(
            UiBuilder::new().max_rect(trailing).layout(Layout::right_to_left(Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if playable {
                    let more = m3::icon_button(ui, IconButtonKind::Standard, Icon::More, false, "More actions");
                    if more.clicked() {
                        ui.ctx().memory_mut(|memory| memory.toggle_popup(popup_id));
                    }
                    egui::popup::popup_below_widget(
                        ui,
                        popup_id,
                        &more,
                        egui::popup::PopupCloseBehavior::CloseOnClick,
                        |ui| {
                            ui.set_min_width(220.0);
                            for (list_index, name) in playlist_names.iter().enumerate() {
                                if ui.button(format!("Add to {name}")).clicked() {
                                    action = RowAction::AddTo(list_index);
                                }
                            }
                            ui.separator();
                            if ui.button("Open on YouTube Music").clicked() {
                                action = RowAction::OpenWeb;
                            }
                            if in_library_view && ui.button("Remove from this playlist").clicked() {
                                action = RowAction::Remove;
                            }
                        },
                    );
                    let like_tooltip = if liked { "Remove from Liked songs" } else { "Add to Liked songs" };
                    if m3::icon_button(ui, IconButtonKind::Standard, Icon::Heart, liked, like_tooltip).clicked() {
                        action = RowAction::ToggleLike;
                    }
                    if m3::icon_button(ui, IconButtonKind::Tonal, Icon::Play, false, "Play").clicked() {
                        action = RowAction::Play;
                    }
                }
                if !narrow {
                    ui.add(egui::Label::new(theme::text(Role::BodyMedium, track.duration_label()).color(subtitle_color)).selectable(false));
                }
            },
        );
        if response.clicked() && playable && matches!(action, RowAction::None) {
            action = RowAction::Play;
        }

        match action {
            RowAction::None => {}
            RowAction::Play => self.play_remote(track),
            RowAction::ToggleLike => {
                let now_liked = self.library.toggle_like(track);
                self.save_library();
                self.notify(if now_liked { "Added to Liked songs." } else { "Removed from Liked songs." });
            }
            RowAction::AddTo(list_index) => {
                let name = self.library.playlists.get(list_index).map(|playlist| playlist.name.clone()).unwrap_or_default();
                let added = self.library.add(list_index, track);
                self.save_library();
                self.notify(if added { format!("Added to \"{name}\".") } else { format!("Already in \"{name}\".") });
            }
            RowAction::Remove => {
                if let Some(video_id) = &track.video_id {
                    self.library.remove(self.selected_playlist, video_id);
                    self.save_library();
                }
            }
            RowAction::OpenWeb => match track.video_id.as_deref().and_then(ytmusic::youtube_music_url) {
                Some(url) => {
                    if let Err(error) = webbrowser::open(&url) {
                        self.notify(format!("Could not open YouTube Music: {error}"));
                    }
                }
                None => self.notify("YouTube returned an invalid video ID."),
            },
        }
        ui.add_space(2.0);
    }

    fn render_home(&mut self, ui: &mut egui::Ui) {
        Self::page_title(ui, "Welcome back", "Find something lovely to listen to.");
        Self::section_title(ui, "Quick picks");
        let tracks = self.state.tracks.clone();
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (index, track) in tracks.iter().enumerate() {
                self.track_row(ui, track, index);
            }
        });
    }

    fn render_search(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        let c = theme::colors(ui);
        Self::page_title(ui, "Search", "Discover songs and artists on YouTube Music.");
        let result = m3::search_bar(ui, &mut self.state.search_query, "Search songs, artists, albums");
        if result.submitted {
            self.start_search(ctx);
        }
        ui.add_space(8.0);
        if let Some(message) = &self.state.search_message {
            ui.label(theme::text(Role::BodyMedium, message).color(c.on_surface_variant));
            ui.add_space(4.0);
        }
        let results = self.state.remote_results.clone();
        if results.is_empty() && !self.search_in_flight {
            m3::empty_state(ui, Icon::Search, "Search for music", "Type a song, artist or album and press Enter.");
            return;
        }
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (index, track) in results.iter().enumerate() {
                self.track_row(ui, track, index);
            }
        });
    }

    fn render_library(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        let c = theme::colors(ui);
        Self::page_title(ui, "Library", "Liked songs and playlists are saved on this computer.");

        ui.horizontal(|ui| {
            let width = (ui.available_width() - 120.0).max(120.0);
            m3::text_field(ui, &mut self.new_playlist_name, "New playlist name", width);
            if m3::button(ui, ButtonKind::Tonal, "Create").clicked() {
                match self.library.create(&self.new_playlist_name) {
                    Some(index) => {
                        self.selected_playlist = index;
                        self.new_playlist_name.clear();
                        self.save_library();
                    }
                    None => self.notify("Enter a playlist name that isn't already used."),
                }
            }
        });
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 120.0).max(120.0);
            m3::text_field(ui, &mut self.import_url, "Import a public playlist link", width);
            let idle = self.import_rx.is_none();
            if m3::button_enabled(ui, ButtonKind::Tonal, "Import", idle).clicked() {
                self.start_import(ctx);
            }
        });
        ui.add_space(8.0);

        if self.selected_playlist >= self.library.playlists.len() {
            self.selected_playlist = 0;
        }
        let labels: Vec<String> = self
            .library
            .playlists
            .iter()
            .map(|playlist| format!("{} ({})", playlist.name, playlist.tracks.len()))
            .collect();
        ui.horizontal_wrapped(|ui| {
            for (index, label) in labels.iter().enumerate() {
                if m3::filter_chip(ui, label, self.selected_playlist == index).clicked() {
                    self.selected_playlist = index;
                }
            }
        });
        ui.add_space(12.0);

        let selected = self.selected_playlist;
        let name = self.library.playlists[selected].name.clone();
        let tracks: Vec<Track> = self.library.playlists[selected].tracks.iter().map(SavedTrack::to_track).collect();
        let mut delete_requested = false;
        ui.horizontal(|ui| {
            ui.label(theme::text(Role::TitleLarge, name).color(c.on_surface));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if m3::button_enabled(ui, ButtonKind::Filled, "Play all", !tracks.is_empty()).clicked() {
                    self.play_playlist(selected);
                }
                if selected > 0 && m3::button(ui, ButtonKind::Destructive, "Delete").clicked() {
                    delete_requested = true;
                }
            });
        });
        if delete_requested {
            self.library.delete(selected);
            self.selected_playlist = 0;
            self.save_library();
            return;
        }
        ui.add_space(8.0);
        if tracks.is_empty() {
            m3::empty_state(
                ui,
                Icon::Library,
                "Nothing here yet",
                "Like songs or add them from search results, or import a playlist link.",
            );
            return;
        }
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for (index, track) in tracks.iter().enumerate() {
                self.track_row(ui, track, index);
            }
        });
    }

    /// A settings row: title and description on the left, a switch on the right.
    /// Returns true when the switch was toggled.
    fn switch_row(ui: &mut egui::Ui, c: &ColorScheme, title: &str, description: &str, on: &mut bool) -> bool {
        let mut changed = false;
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            changed = m3::switch(ui, on).changed();
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.label(theme::text(Role::BodyLarge, title).color(c.on_surface));
                ui.label(theme::text(Role::BodyMedium, description).color(c.on_surface_variant));
            });
        });
        changed
    }

    fn render_settings(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        let c = theme::colors(ui);
        Self::page_title(ui, "Settings", "Manage appearance and playback.");
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            m3::card(ui, CardKind::Filled, |ui| {
                ui.label(theme::text(Role::TitleMedium, "Appearance").color(c.on_surface));
                ui.label(theme::text(Role::BodyMedium, "Choose a light or dark theme, or follow your system.").color(c.on_surface_variant));
                ui.add_space(12.0);
                let current = match self.theme_pref {
                    ThemePreference::System => 0,
                    ThemePreference::Light => 1,
                    ThemePreference::Dark => 2,
                };
                if let Some(choice) = m3::segmented(ui, &["System", "Light", "Dark"], current) {
                    self.theme_pref = match choice {
                        1 => ThemePreference::Light,
                        2 => ThemePreference::Dark,
                        _ => ThemePreference::System,
                    };
                    ctx.set_theme(self.theme_pref);
                    theme::save_preference(self.theme_pref);
                }
            });
            ui.add_space(12.0);
            m3::card(ui, CardKind::Filled, |ui| {
                ui.label(theme::text(Role::TitleMedium, "Background and notifications").color(c.on_surface));
                ui.add_space(8.0);
                let mut notifications_on = self.settings.notifications;
                let mut tray_on = self.tray.is_some();
                let mut changed = Self::switch_row(
                    ui, c, "Track notifications",
                    "Show the song when a new one starts while TweMusic is in the background.",
                    &mut notifications_on,
                );
                ui.add_space(12.0);
                changed |= Self::switch_row(
                    ui, c, "Keep playing in the tray",
                    "Closing the window minimises it and shows a tray icon with controls. Quit from the tray menu.",
                    &mut tray_on,
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if m3::button(ui, ButtonKind::Outlined, "Send test notification").clicked() {
                        let tx = self.notice_tx.clone();
                        let wake = ctx.clone();
                        notifications::send_test(move |error| {
                            let _ = tx.send(error.unwrap_or_else(|| "Test notification sent.".to_owned()));
                            wake.request_repaint();
                        });
                    }
                    if m3::button(ui, ButtonKind::Tonal, "Quit TweMusic").clicked() {
                        self.quit(ctx);
                    }
                });
                if changed {
                    self.settings.notifications = notifications_on;
                    if tray_on {
                        self.settings.tray = self.start_tray(ctx);
                    } else {
                        self.stop_tray();
                        self.settings.tray = false;
                    }
                    self.save_settings();
                }
            });
            ui.add_space(12.0);
            m3::card(ui, CardKind::Filled, |ui| {
                ui.label(theme::text(Role::TitleMedium, "Local audio playback").color(c.on_surface));
                ui.label(theme::text(Role::BodyMedium, "Play a file from your device using MPV.").color(c.on_surface_variant));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let width = (ui.available_width() - 130.0).max(120.0);
                    m3::text_field(ui, &mut self.local_file_path, "/path/to/audio-file", width);
                    if m3::button(ui, ButtonKind::Filled, "Play file").clicked() {
                        self.start_local_playback();
                    }
                });
            });
            ui.add_space(12.0);
            m3::card(ui, CardKind::Filled, |ui| {
                ui.label(theme::text(Role::TitleMedium, "YouTube Music").color(c.on_surface));
                ui.add_space(4.0);
                ui.label(theme::text(Role::BodyMedium, "Search returns public metadata. Play streams the audio only, through MPV and yt-dlp, so no video or ads load.").color(c.on_surface_variant));
                ui.add_space(4.0);
                ui.label(theme::text(Role::BodyMedium, "Requires mpv and yt-dlp. This may go against YouTube's terms of service, and yt-dlp needs regular updates.").color(c.on_surface_variant));
            });
        });
    }

    fn render_top_bar(&mut self, ctx: &Context, c: &ColorScheme, margin: f32) {
        let title = match self.state.screen {
            Screen::Home => "Home",
            Screen::Search => "Search",
            Screen::Library => "Library",
            Screen::Settings => "Settings",
        };
        TopBottomPanel::top("top-app-bar")
            .exact_height(64.0)
            .show_separator_line(false)
            .frame(Frame::none().fill(c.surface).inner_margin(Margin::symmetric(margin, 0.0)))
            .show(ctx, |ui| {
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
                    ui.painter().circle_filled(rect.center(), 16.0, c.primary_container);
                    Icon::Note.paint(ui.painter(), rect.center(), 18.0, c.on_primary_container, false);
                    ui.add_space(8.0);
                    ui.label(theme::text(Role::TitleLarge, title).color(c.on_surface));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if m3::icon_button(ui, IconButtonKind::Standard, Icon::Power, false, "Quit TweMusic").clicked() {
                            self.quit(ctx);
                        }
                        let mode = match self.theme_pref {
                            ThemePreference::System => "system",
                            ThemePreference::Light => "light",
                            ThemePreference::Dark => "dark",
                        };
                        let tooltip = format!("Theme: {mode} (click to change)");
                        if m3::icon_button(ui, IconButtonKind::Standard, Icon::Contrast, false, &tooltip).clicked() {
                            self.theme_pref = theme::next_preference(self.theme_pref);
                            ctx.set_theme(self.theme_pref);
                            theme::save_preference(self.theme_pref);
                        }
                    });
                });
            });
    }

    fn destinations() -> [(Screen, Icon, &'static str); 4] {
        [
            (Screen::Home, Icon::Home, "Home"),
            (Screen::Search, Icon::Search, "Search"),
            (Screen::Library, Icon::Library, "Library"),
            (Screen::Settings, Icon::Settings, "Settings"),
        ]
    }

    /// Navigation rail for medium and expanded windows.
    fn render_nav_rail(&mut self, ctx: &Context, c: &ColorScheme) {
        SidePanel::left("navigation-rail")
            .exact_width(80.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::none().fill(c.surface).inner_margin(Margin::symmetric(0.0, 12.0)))
            .show(ctx, |ui| {
                for (screen, icon, label) in Self::destinations() {
                    if m3::rail_destination(ui, icon, label, self.state.screen == screen).clicked() {
                        self.state.screen = screen;
                    }
                }
            });
    }

    /// Navigation bar for compact windows.
    fn render_nav_bar(&mut self, ctx: &Context, c: &ColorScheme) {
        TopBottomPanel::bottom("navigation-bar")
            .exact_height(80.0)
            .show_separator_line(false)
            .frame(Frame::none().fill(c.surface_container))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let width = ui.available_width() / 4.0;
                    for (screen, icon, label) in Self::destinations() {
                        if m3::bar_destination(ui, width, icon, label, self.state.screen == screen).clicked() {
                            self.state.screen = screen;
                        }
                    }
                });
            });
    }

    fn render_player(&mut self, ctx: &Context, c: &ColorScheme, margin: f32) {
        let (title, subtitle) = match self.state.current_track() {
            Some(track) if self.mpv.is_some() => (track.title.clone(), track.artist.clone()),
            _ if self.mpv.is_some() => ("Local audio player".to_owned(), "MPV".to_owned()),
            _ => ("Nothing playing".to_owned(), "Search for a song to start".to_owned()),
        };
        let status = self.player_message.clone().unwrap_or(subtitle);
        let progress = match (self.position, self.duration) {
            (Some(position), Some(duration)) if duration > 0.0 => Some((position, duration)),
            _ => None,
        };
        let can_prev = self.state.adjacent_remote(false).is_some();
        let can_next = self.state.adjacent_remote(true).is_some();
        let playing = self.state.playback == PlaybackStatus::Playing;
        let art = self
            .state
            .current_track()
            .filter(|_| self.mpv.is_some())
            .and_then(|track| track.thumbnail_url.clone());
        let mut seek_event = SeekEvent::Idle;

        TopBottomPanel::bottom("player-panel")
            .show_separator_line(false)
            .frame(Frame::none().fill(c.surface).inner_margin(Margin { left: margin, right: margin, top: 8.0, bottom: 8.0 }))
            .show(ctx, |ui| {
                Frame::none()
                    .fill(c.surface_container_high)
                    .rounding(Rounding::same(shape::LARGE))
                    .inner_margin(Margin::same(12.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let (thumb, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
                            ui.painter().rect_filled(thumb, Rounding::same(shape::SMALL), c.primary_container);
                            Icon::Note.paint(ui.painter(), thumb.center(), 24.0, c.on_primary_container, false);
                            if let Some(url) = &art {
                                paint_album_art(ui, url, thumb);
                            }

                            let text_width = (ui.available_width() - 12.0 - 40.0 - 48.0 - 40.0 - 24.0).max(60.0);
                            ui.allocate_ui_with_layout(vec2(text_width, 48.0), Layout::top_down(Align::Min), |ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                ui.add(egui::Label::new(theme::text(Role::TitleMedium, &title).color(c.on_surface)).truncate().selectable(false));
                                ui.add(egui::Label::new(theme::text(Role::BodyMedium, &status).color(c.on_surface_variant)).truncate().selectable(false));
                            });

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if m3::icon_button_ex(ui, IconButtonKind::Standard, Icon::Next, false, can_next, 40.0, "Next").clicked() {
                                    self.skip(true);
                                }
                                let (icon, tooltip) = if playing { (Icon::Pause, "Pause") } else { (Icon::Play, "Play") };
                                if m3::icon_button_ex(ui, IconButtonKind::Filled, icon, false, true, 48.0, tooltip).clicked() {
                                    self.toggle_local_playback();
                                }
                                if m3::icon_button_ex(ui, IconButtonKind::Standard, Icon::Previous, false, can_prev, 40.0, "Previous").clicked() {
                                    self.skip(false);
                                }
                            });
                        });

                        ui.horizontal(|ui| {
                            let (position, duration) = progress.unwrap_or((0.0, 0.0));
                            let shown = self.seek_drag.unwrap_or(position).clamp(0.0, duration.max(0.0));
                            ui.add_sized(
                                [44.0, 32.0],
                                egui::Label::new(theme::text(Role::LabelMedium, format_seconds(shown)).color(c.on_surface_variant)).selectable(false),
                            );
                            let bar_width = (ui.available_width() - 44.0 - ui.spacing().item_spacing.x).max(60.0);
                            seek_event = m3::seek_bar(ui, shown, duration, bar_width, progress.is_some());
                            ui.add_sized(
                                [44.0, 32.0],
                                egui::Label::new(theme::text(Role::LabelMedium, format_seconds(duration)).color(c.on_surface_variant)).selectable(false),
                            );
                        });
                    });
            });

        match seek_event {
            SeekEvent::Idle => {}
            SeekEvent::Dragging(value) => self.seek_drag = Some(value),
            SeekEvent::Committed(value) => {
                self.seek_drag = None;
                self.seek_to(value);
            }
        }
    }
}

impl eframe::App for TweMusicApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.poll_search();
        self.poll_media();
        self.poll_tray(ctx);
        self.handle_close(ctx);
        self.poll_import();
        self.tick_player(ctx);

        let c = theme::colors_ctx(ctx);
        let size = WindowSize::from_width(ctx.screen_rect().width());
        let margin = size.margin();

        self.render_top_bar(ctx, c, margin);
        if size == WindowSize::Compact {
            self.render_nav_bar(ctx, c);
        } else {
            self.render_nav_rail(ctx, c);
        }
        self.render_player(ctx, c, margin);

        let content_rect = ctx.available_rect();
        CentralPanel::default()
            .frame(Frame::none().fill(c.surface).inner_margin(Margin::symmetric(margin, 0.0)))
            .show(ctx, |ui| match self.state.screen {
                Screen::Home => self.render_home(ui),
                Screen::Search => self.render_search(ui, ctx),
                Screen::Library => self.render_library(ui, ctx),
                Screen::Settings => self.render_settings(ui, ctx),
            });

        let expired = self.snackbar.as_ref().is_some_and(|(_, since)| since.elapsed() >= SNACKBAR_TIME);
        if expired {
            self.snackbar = None;
        }
        if let Some((message, _)) = &self.snackbar {
            m3::snackbar(ctx, content_rect, message);
            ctx.request_repaint_after(Duration::from_millis(250));
        }

        self.sync_media();
        self.sync_tray();
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 760.0])
            .with_min_inner_size([360.0, 480.0])
            .with_title("TweMusic"),
        ..Default::default()
    };
    eframe::run_native("TweMusic", options, Box::new(|cc| Ok(Box::new(TweMusicApp::new(cc.egui_ctx.clone())))))
}
