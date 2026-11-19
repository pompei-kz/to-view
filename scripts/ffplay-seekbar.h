/* Included by ffplay.c after the playback clock and seek functions. */

enum { SEEK_GLYPH_SCALE = 2, SEEK_GLYPH_HEIGHT = 7, SEEK_GLYPH_ADVANCE = 12 };

static int64_t seekbar_duration(const VideoState *is)
{
    return is->ic && is->ic->duration > 0 ? is->ic->duration : -1;
}

static int64_t seekbar_start(const VideoState *is)
{
    return is->ic && is->ic->start_time != AV_NOPTS_VALUE ? is->ic->start_time : 0;
}

static SDL_Rect seekbar_track(void)
{
    int width, height;
    if (SDL_GetRendererOutputSize(renderer, &width, &height) < 0 ||
        width < 3 || height < 3)
        return (SDL_Rect){ 0 };

    int bar_height = FFMIN(height, SEEK_GLYPH_HEIGHT * SEEK_GLYPH_SCALE + 14);
    return (SDL_Rect){ 0, height - bar_height, width, bar_height };
}

static int seekbar_hit(SDL_Rect track, int x, int y, int *output_x)
{
    int width, height;
    SDL_GetWindowSize(window, &width, &height);
    if (width <= 0 || height <= 0 || !track.w ||
        x < 0 || x >= width || y < 0 || y >= height ||
        (int64_t)y * (track.y + track.h) / height < track.y)
        return 0;

    *output_x = width > 1 ? (int)av_rescale(x, track.w - 1, width - 1) : 0;
    return 1;
}

static int64_t seekbar_target(SDL_Rect track, int x, int64_t duration)
{
    return av_rescale(x, duration, track.w - 1);
}

static void seekbar_format(char *text, size_t capacity, int64_t us)
{
    if (us < 0) {
        snprintf(text, capacity, "~");
        return;
    }

    int64_t tenths = us / (AV_TIME_BASE / 10);
    int64_t hours = tenths / 36000;
    int minutes = tenths / 600 % 60;
    int seconds = tenths / 10 % 60;
    int fraction = tenths % 10;

    if (hours)
        snprintf(text, capacity, "%02" PRId64 ":%02d:%02d.%d", hours, minutes, seconds, fraction);
    else if (minutes)
        snprintf(text, capacity, "%02d:%02d.%d", minutes, seconds, fraction);
    else
        snprintf(text, capacity, "%02d.%d", seconds, fraction);
}

static const uint8_t *seekbar_glyph(char ch)
{
    static const uint8_t digits[10][7] = {
        { 14, 17, 19, 21, 25, 17, 14 },
        {  4, 12,  4,  4,  4,  4, 14 },
        { 14, 17,  1,  2,  4,  8, 31 },
        { 30,  1,  1, 14,  1,  1, 30 },
        {  2,  6, 10, 18, 31,  2,  2 },
        { 31, 16, 16, 30,  1,  1, 30 },
        { 14, 16, 16, 30, 17, 17, 14 },
        { 31,  1,  2,  4,  8,  8,  8 },
        { 14, 17, 17, 14, 17, 17, 14 },
        { 14, 17, 17, 15,  1,  1, 14 },
    };
    static const uint8_t colon[7] = { 0, 0, 4, 0, 4, 0, 0 };
    static const uint8_t dot[7]   = { 0, 0, 0, 0, 0, 0, 4 };
    static const uint8_t dash[7]  = { 0, 0, 0, 14, 0, 0, 0 };
    static const uint8_t arrow[7] = { 0, 8, 4, 2, 4, 8, 0 };
    static const uint8_t blank[7] = { 0 };

    if (ch >= '0' && ch <= '9')
        return digits[ch - '0'];
    switch (ch) {
    case ':': return colon;
    case '.': return dot;
    case '-':
    case '~': return dash;
    case '>': return arrow;
    default:  return blank;
    }
}

static void seekbar_text(const char *text, int x, int y)
{
    /* Draw one label at a time so its outline also covers lower layers. */
    for (int outline = 1; outline >= 0; outline--) {
        SDL_SetRenderDrawColor(renderer, outline ? 16 : 255, outline ? 24 : 255,
                              outline ? 32 : 255, 255);
        for (const char *p = text; *p; p++) {
            SDL_Rect pixels[5 * SEEK_GLYPH_HEIGHT];
            int count = 0;
            const uint8_t *glyph = seekbar_glyph(*p);
            for (int row = 0; row < SEEK_GLYPH_HEIGHT; row++) {
                for (int col = 0; col < 5; col++) {
                    if (!(glyph[row] & (16 >> col)))
                        continue;
                    pixels[count++] = (SDL_Rect){
                        x + (int)(p - text) * SEEK_GLYPH_ADVANCE + col * SEEK_GLYPH_SCALE - outline,
                        y + row * SEEK_GLYPH_SCALE - outline,
                        SEEK_GLYPH_SCALE + 2 * outline, SEEK_GLYPH_SCALE + 2 * outline,
                    };
                }
            }
            if (count)
                SDL_RenderFillRects(renderer, pixels, count);
        }
    }
}

static void seekbar_marker(SDL_Rect track, int x, int dashed)
{
    int bottom = track.y + track.h;
    int stride = dashed ? 8 : track.h;
    for (int y = track.y; y < bottom; y += stride) {
        int remaining = bottom - y;
        int h = dashed && remaining > 8 ? 4 : remaining;
        SDL_Rect stroke = { x - 1, y, 3, h };
        SDL_SetRenderDrawColor(renderer, 16, 24, 32, 255);
        SDL_RenderFillRect(renderer, &stroke);
        SDL_SetRenderDrawColor(renderer, 255, 255, 255, 255);
        SDL_RenderDrawLine(renderer, x, y, x, y + h - 1);
    }
}

static int64_t seekbar_position(VideoState *is, int64_t duration, int ended)
{
    if (duration > 0 && ended)
        return duration;

    double clock = get_master_clock(is);
    int64_t position = isfinite(clock) ? FFMAX(0, clock * AV_TIME_BASE - seekbar_start(is)) : 0;
    return duration > 0 ? FFMIN(position, duration) : position;
}

static void seekbar_seek_to(VideoState *is, int64_t position, int64_t duration,
                            int boundary)
{
    int64_t start = seekbar_start(is);
    position = FFMAX(position, 0);
    if (duration > 0)
        position = FFMIN(position, duration - 1); /* End is after the last sample. */
    int64_t target = av_sat_add64(start, position);
    AVStream *video = is->video_st;
    if (boundary > 0 && video && video->start_time != AV_NOPTS_VALUE &&
        video->duration > 0) {
        /* The video may end before a longer audio track. */
        int64_t video_end = av_rescale_q(av_sat_add64(video->start_time, video->duration),
                                        video->time_base, AV_TIME_BASE_Q);
        target = FFMIN(target, av_sat_sub64(video_end, 1));
    }

    stream_seek_internal(is, target, 0, 0, 1, boundary);
    is->force_refresh = 1;
}

static void seekbar_seek_key(VideoState *is, const SDL_Keysym *key)
{
    int64_t duration = seekbar_duration(is);
    int ended = SDL_AtomicGet(&is->seekbar_ended);
    int64_t position = seekbar_position(is, duration, ended);
    int64_t target;
    if (key->sym == SDLK_HOME) {
        target = 0;
    } else if (key->sym == SDLK_END) {
        if (duration <= 0)
            return;
        target = duration;
    } else {
        int seconds = 1;
        if (key->mod & KMOD_SHIFT)
            seconds = (key->mod & KMOD_ALT) ? 30 : 5;
        int64_t offset = (int64_t)seconds * AV_TIME_BASE;
        target = av_sat_add64(position, key->sym == SDLK_LEFT ? -offset : offset);
    }

    int boundary = target <= 0 ? -1 : (duration > 0 && target >= duration ? 1 : 0);
    if ((boundary < 0 && is->paused && position == 0) || (boundary > 0 && ended))
        return;
    seekbar_seek_to(is, target, duration, boundary);
}

static void seekbar_draw(VideoState *is)
{
    SDL_Rect track = seekbar_track();
    if (!track.w)
        return;

    int64_t duration = seekbar_duration(is);
    is->seekbar_drawn_end = SDL_AtomicGet(&is->seekbar_ended);
    int64_t position = seekbar_position(is, duration, is->seekbar_drawn_end);

    char current[32], total[32];
    seekbar_format(current, sizeof(current), position);
    seekbar_format(total, sizeof(total), duration);

    int mouse_x, mouse_y, hover_x;
    SDL_GetMouseState(&mouse_x, &mouse_y);
    int hover = duration > 0 && SDL_GetMouseFocus() == window &&
                seekbar_hit(track, mouse_x, mouse_y, &hover_x);
    int text_y = track.y + (track.h - SEEK_GLYPH_HEIGHT * SEEK_GLYPH_SCALE) / 2;
    const int label_inset = 8;
    if (hover) {
        char target[36] = " -> ";
        seekbar_format(target + 4, sizeof(target) - 4, seekbar_target(track, hover_x, duration));
        seekbar_text(target, label_inset + (int)strlen(current) * SEEK_GLYPH_ADVANCE, text_y);
    }
    seekbar_text(current, label_inset, text_y);
    seekbar_text(total, track.w - label_inset - (int)strlen(total) * SEEK_GLYPH_ADVANCE, text_y);

    SDL_SetRenderDrawColor(renderer, 16, 24, 32, 255);
    SDL_RenderDrawRect(renderer, &track);
    SDL_Rect inner = { track.x + 1, track.y + 1, track.w - 2, track.h - 2 };
    SDL_SetRenderDrawColor(renderer, 255, 255, 255, 255);
    SDL_RenderDrawRect(renderer, &inner);
    if (duration > 0)
        seekbar_marker(track, (int)av_rescale(position, track.w - 1, duration), 0);
    if (hover)
        seekbar_marker(track, hover_x, 1);
}

static int seekbar_click(VideoState *is, const SDL_MouseButtonEvent *button)
{
    int x;
    if (!is->menu_active || button->button != SDL_BUTTON_LEFT)
        return 0;
    SDL_Rect track = seekbar_track();
    if (!seekbar_hit(track, button->x, button->y, &x))
        return 0;

    int64_t duration = seekbar_duration(is);
    if (duration > 0)
        seekbar_seek_to(is, seekbar_target(track, x, duration), duration, 0);
    return 1;
}
