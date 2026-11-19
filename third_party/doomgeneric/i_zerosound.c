//
// Sound for the Zero kernel: a sound effect mixer and an OPL2 (AdLib/Sound
// Blaster FM) synthesizer that plays DOOM's own music.
//
// DOOM's music is stored as MUS scores, and the instruments are FM patches in
// the GENMIDI lump of the WAD: exactly what an AdLib card played in 1993. This
// file reads both and renders them with a small software OPL2, so the tunes
// sound like the original game rather than like a General MIDI bank.
//
// Everything is integer arithmetic (the kernel has no floating point unit).
// The kernel's audio driver calls DG_MixAudio() whenever it needs more
// samples: 48000 Hz, signed 16 bit, stereo.
//
// This file is part of the Zero kernel port, licensed under the GNU GPL v2
// like the rest of the engine.
//

#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>

#include "doomtype.h"
#include "i_sound.h"
#include "sounds.h"
#include "w_wad.h"
#include "z_zone.h"

#define MIX_RATE 48000
#define OPL_RATE 49716

// ---------------------------------------------------------------- sound effects

#define NUM_CHANNELS 16

typedef struct
{
    const uint8_t *data;   // unsigned 8 bit samples
    uint32_t length;
    uint32_t pos;          // 16.16 fixed point index into data
    uint32_t step;         // 16.16 samples per output frame
    int left, right;       // 0..127
    int active;
} channel_t;

static channel_t channels[NUM_CHANNELS];
static int sfx_ready = 0;

static snddevice_t sfx_devices[] = { SNDDEVICE_SB, SNDDEVICE_PAS, SNDDEVICE_GUS, SNDDEVICE_AWE32 };

static boolean Sfx_Init(boolean use_sfx_prefix)
{
    memset(channels, 0, sizeof(channels));
    sfx_ready = 1;
    return true;
}

static void Sfx_Shutdown(void)
{
    sfx_ready = 0;
}

static int use_prefix = 1;

static int Sfx_GetLumpNum(sfxinfo_t *sfx)
{
    char name[16];

    if (sfx->link != NULL)
    {
        sfx = sfx->link;
    }
    snprintf(name, sizeof(name), "ds%s", sfx->name);
    return W_GetNumForName(name);
}

static void Sfx_Update(void)
{
}

static void SetVolumes(channel_t *c, int vol, int sep)
{
    // the classic DMX stereo law
    int left = vol - ((vol * sep * sep) / (256 * 256));
    sep -= 257;
    int right = vol - ((vol * sep * sep) / (256 * 256));

    c->left = left < 0 ? 0 : (left > 127 ? 127 : left);
    c->right = right < 0 ? 0 : (right > 127 ? 127 : right);
}

static void Sfx_UpdateParams(int channel, int vol, int sep)
{
    if (channel >= 0 && channel < NUM_CHANNELS)
    {
        SetVolumes(&channels[channel], vol, sep);
    }
}

static int Sfx_Start(sfxinfo_t *sfx, int channel, int vol, int sep)
{
    uint8_t *lump;
    int lumpnum = sfx->lumpnum;
    int length, rate, samples;

    if (!sfx_ready || channel < 0 || channel >= NUM_CHANNELS)
    {
        return -1;
    }

    lump = W_CacheLumpNum(lumpnum, PU_STATIC);
    length = W_LumpLength(lumpnum);

    // DMX sound: format 3, rate, sample count, 16 bytes of padding, samples
    if (length < 8 || lump[0] != 3 || lump[1] != 0)
    {
        return -1;
    }
    rate = lump[2] | (lump[3] << 8);
    samples = lump[4] | (lump[5] << 8) | (lump[6] << 16) | (lump[7] << 24);
    if (samples > length - 8)
    {
        samples = length - 8;
    }
    if (samples <= 32 || rate <= 0)
    {
        return -1;
    }

    channels[channel].data = lump + 8 + 16;
    channels[channel].length = samples - 32;
    channels[channel].pos = 0;
    channels[channel].step = (uint32_t) (((uint64_t) rate << 16) / MIX_RATE);
    SetVolumes(&channels[channel], vol, sep);
    channels[channel].active = 1;
    return channel;
}

static void Sfx_Stop(int channel)
{
    if (channel >= 0 && channel < NUM_CHANNELS)
    {
        channels[channel].active = 0;
    }
}

static boolean Sfx_IsPlaying(int channel)
{
    return channel >= 0 && channel < NUM_CHANNELS && channels[channel].active;
}

static void Sfx_Cache(sfxinfo_t *sounds, int num_sounds)
{
}

sound_module_t DG_sound_module =
{
    sfx_devices,
    sizeof(sfx_devices) / sizeof(*sfx_devices),
    Sfx_Init,
    Sfx_Shutdown,
    Sfx_GetLumpNum,
    Sfx_Update,
    Sfx_UpdateParams,
    Sfx_Start,
    Sfx_Stop,
    Sfx_IsPlaying,
    Sfx_Cache,
};

// ---------------------------------------------------------------- OPL2 emulation

#define NUM_VOICES 9

// attenuation runs 0 (loudest) to 511 (silent), 0.1875 dB per step
static int16_t sine_table[1024];       // one cycle, +-4095
static uint32_t gain_table[512];       // 16.16 linear gain for each step
static uint16_t velocity_att[128];     // attenuation for a 0..127 volume
static int tables_ready = 0;

enum { EG_OFF, EG_ATTACK, EG_DECAY, EG_SUSTAIN, EG_RELEASE };

typedef struct
{
    uint8_t r20, r40, r60, r80, rE0;   // the OPL registers this operator uses
    int state;
    uint32_t phase;                    // 20 bit accumulator
    uint32_t att16;                    // attenuation, 16.16
    int out, prev;
} operator_t;

typedef struct
{
    operator_t op[2];                  // modulator, carrier
    uint8_t c0;                        // feedback and connection
    uint32_t fnum;
    int block;
    int keyon;
    int note, music_channel;
    uint32_t age;
    int velocity_att;
    int out;
} voice_t;

static voice_t voices[NUM_VOICES];
static uint32_t voice_age = 0;

static void InitTables(void)
{
    int i, j;

    if (tables_ready)
    {
        return;
    }

    // a half cycle of sine from a parabola with one correction term; the
    // error is well under 0.1%, which nobody hears
    for (j = 0; j < 512; ++j)
    {
        int64_t y = (int64_t) j * (512 - j);               // 0..65536
        int64_t y2 = y + (225 * (((y * y) >> 16) - y)) / 1000;
        int v = (int) ((y2 * 4095) >> 16);

        sine_table[j] = v;
        sine_table[512 + j] = -v;
    }

    gain_table[0] = 65536;
    for (i = 1; i < 512; ++i)
    {
        gain_table[i] = (uint32_t) (((uint64_t) gain_table[i - 1] * 64136) >> 16);
    }

    velocity_att[0] = 511;
    for (i = 1; i < 128; ++i)
    {
        uint32_t target = (uint32_t) (65536 * i / 127);

        for (j = 0; j < 511 && gain_table[j] > target; ++j)
        {
        }
        velocity_att[i] = j;
    }

    tables_ready = 1;
}

// how fast the envelope moves for an effective rate of 0..63
static uint32_t DecayStep(int idx)
{
    int r = idx >> 2, f = idx & 3;
    uint32_t base = (18u * (4 + f)) >> 2;

    if (idx <= 0)
    {
        return 0;
    }
    if (r < 1)
    {
        return base >> 1;
    }
    return base << (r - 1);
}

static uint32_t AttackK(int idx)
{
    int r = idx >> 2, f = idx & 3;
    uint32_t base = (3u * (4 + f)) >> 2;

    if (idx <= 0)
    {
        return 0;
    }
    if (r < 1)
    {
        return 1;
    }
    return base << (r - 1);
}

static int EffectiveRate(int rate, voice_t *v, int ksr)
{
    int rof = (v->block << 1) | ((v->fnum >> 9) & 1);

    if (rate == 0)
    {
        return 0;
    }
    if (!ksr)
    {
        rof >>= 2;
    }
    rate = rate * 4 + rof;
    return rate > 63 ? 63 : rate;
}

static void StepEnvelope(operator_t *o, voice_t *v)
{
    int ksr = (o->r20 >> 4) & 1;

    switch (o->state)
    {
        case EG_ATTACK:
        {
            int idx = EffectiveRate(o->r60 >> 4, v, ksr);

            if (idx >= 60)
            {
                o->att16 = 0;
            }
            else if (idx > 0)
            {
                o->att16 -= (uint32_t) (((uint64_t) o->att16 * AttackK(idx)) >> 16);
            }
            if (o->att16 < 65536)
            {
                o->att16 = 0;
                o->state = EG_DECAY;
            }
            break;
        }
        case EG_DECAY:
        {
            int sl = o->r80 >> 4;
            uint32_t level = (uint32_t) (sl == 15 ? 496 : sl * 16) << 16;

            o->att16 += DecayStep(EffectiveRate(o->r60 & 15, v, ksr));
            if (o->att16 >= level)
            {
                o->att16 = level;
                o->state = EG_SUSTAIN;
            }
            break;
        }
        case EG_SUSTAIN:
            if (!(o->r20 & 0x20))
            {
                o->att16 += DecayStep(EffectiveRate(o->r80 & 15, v, ksr));
                if (o->att16 >= (511u << 16))
                {
                    o->att16 = 511u << 16;
                    o->state = EG_OFF;
                }
            }
            break;
        case EG_RELEASE:
            o->att16 += DecayStep(EffectiveRate(o->r80 & 15, v, ksr));
            if (o->att16 >= (511u << 16))
            {
                o->att16 = 511u << 16;
                o->state = EG_OFF;
            }
            break;
        default:
            break;
    }
}

static int Wave(int form, int ph)
{
    switch (form & 3)
    {
        case 0:  return sine_table[ph];
        case 1:  return ph < 512 ? sine_table[ph] : 0;
        case 2:  return sine_table[ph & 511];
        default: return (ph & 511) < 256 ? sine_table[ph & 255] : 0;
    }
}

static const uint8_t multiples[16] = { 1, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 20, 24, 24, 30, 30 };

static int OperatorOut(operator_t *o, voice_t *v, int modulation, int extra_att)
{
    uint32_t ph;
    int att, s;

    o->phase += (uint32_t) (((v->fnum << v->block) * multiples[o->r20 & 15]) >> 1);
    ph = ((o->phase >> 10) + modulation) & 1023;
    s = Wave(o->rE0, ph);
    att = (int) (o->att16 >> 16) + ((o->r40 & 63) << 2) + extra_att;
    if (att > 511)
    {
        att = 511;
    }
    return (int) (((int64_t) s * gain_table[att]) >> 16);
}

static void StepVoice(voice_t *v)
{
    operator_t *m = &v->op[0], *c = &v->op[1];
    int fb = (v->c0 >> 1) & 7;
    int fbmod = fb ? (m->prev + m->out) >> (9 - fb) : 0;
    int additive = v->c0 & 1;

    if (m->state == EG_OFF && c->state == EG_OFF)
    {
        v->out = 0;
        return;
    }
    StepEnvelope(m, v);
    StepEnvelope(c, v);

    m->prev = m->out;
    m->out = OperatorOut(m, v, fbmod, additive ? v->velocity_att : 0);
    c->out = OperatorOut(c, v, additive ? 0 : (m->out >> 1), v->velocity_att);
    v->out = additive ? (m->out + c->out) >> 1 : c->out;
}

static int OplSample(void)
{
    int i, sum = 0;

    for (i = 0; i < NUM_VOICES; ++i)
    {
        StepVoice(&voices[i]);
        sum += voices[i].out;
    }
    return sum;
}

// ---------------------------------------------------------------- instruments (GENMIDI)

typedef struct
{
    uint8_t tremolo, attack, sustain, waveform, scale, level;
} genmidi_op_t;

typedef struct
{
    genmidi_op_t modulator;
    uint8_t feedback;
    genmidi_op_t carrier;
    uint8_t unused;
    int16_t base_note_offset;
} genmidi_voice_t;

typedef struct
{
    uint16_t flags;
    uint8_t fine_tuning;
    uint8_t fixed_note;
    genmidi_voice_t voices[2];
} genmidi_instr_t;

// the file layout is packed; read the fields by hand
static const uint8_t *genmidi = NULL;

static void ReadInstrument(int index, genmidi_instr_t *out)
{
    const uint8_t *p = genmidi + 8 + index * 36;
    int i;

    out->flags = p[0] | (p[1] << 8);
    out->fine_tuning = p[2];
    out->fixed_note = p[3];
    p += 4;
    for (i = 0; i < 2; ++i)
    {
        genmidi_voice_t *v = &out->voices[i];

        v->modulator.tremolo = p[0];
        v->modulator.attack = p[1];
        v->modulator.sustain = p[2];
        v->modulator.waveform = p[3];
        v->modulator.scale = p[4];
        v->modulator.level = p[5];
        v->feedback = p[6];
        v->carrier.tremolo = p[7];
        v->carrier.attack = p[8];
        v->carrier.sustain = p[9];
        v->carrier.waveform = p[10];
        v->carrier.scale = p[11];
        v->carrier.level = p[12];
        v->unused = p[13];
        v->base_note_offset = (int16_t) (p[14] | (p[15] << 8));
        p += 16;
    }
}

// F-number of each note in octave 4 (block 4)
static const uint16_t fnum_table[12] = { 345, 365, 387, 410, 435, 460, 488, 517, 547, 580, 615, 651 };

typedef struct
{
    int instrument;
    int volume;       // 0..127
    int bend;         // 0..255, 128 is centre
    int last_velocity;
} music_channel_t;

static music_channel_t mchan[16];
static int music_volume = 127;

static void SetFrequency(voice_t *v, int note, int bend)
{
    int block, fnum;

    if (note < 0) note = 0;
    if (note > 127) note = 127;
    block = note / 12 - 1;
    fnum = fnum_table[note % 12];
    if (block < 0)
    {
        fnum >>= 1;
        block = 0;
    }
    if (block > 7)
    {
        block = 7;
    }
    fnum += fnum * (bend - 128) * 122 / (128 * 1000);
    if (fnum > 1023) fnum = 1023;
    v->fnum = fnum;
    v->block = block;
}

static void LoadOperator(operator_t *o, const genmidi_op_t *src)
{
    o->r20 = src->tremolo;
    o->r40 = src->level;                     // key scale is not emulated
    o->r60 = src->attack;
    o->r80 = src->sustain;
    o->rE0 = src->waveform;
}

static void KeyOff(voice_t *v)
{
    int i;

    v->keyon = 0;
    for (i = 0; i < 2; ++i)
    {
        if (v->op[i].state != EG_OFF)
        {
            v->op[i].state = EG_RELEASE;
        }
    }
}

static voice_t *AllocateVoice(void)
{
    voice_t *best = NULL;
    int i;

    for (i = 0; i < NUM_VOICES; ++i)
    {
        if (voices[i].op[1].state == EG_OFF)
        {
            return &voices[i];
        }
    }
    // steal the oldest released note, else the oldest note
    for (i = 0; i < NUM_VOICES; ++i)
    {
        if (!voices[i].keyon && (best == NULL || voices[i].age < best->age))
        {
            best = &voices[i];
        }
    }
    if (best != NULL)
    {
        return best;
    }
    best = &voices[0];
    for (i = 1; i < NUM_VOICES; ++i)
    {
        if (voices[i].age < best->age)
        {
            best = &voices[i];
        }
    }
    return best;
}

static int VoiceAttenuation(int velocity, int channel_volume)
{
    int v = velocity * channel_volume / 127;

    return velocity_att[v > 127 ? 127 : v];
}

static void NoteOn(int channel, int note, int velocity)
{
    genmidi_instr_t instr;
    genmidi_voice_t *patch;
    voice_t *v;
    int index, pitch;

    if (genmidi == NULL)
    {
        return;
    }
    if (velocity < 0)
    {
        velocity = mchan[channel].last_velocity;
    }
    mchan[channel].last_velocity = velocity;

    if (channel == 15)
    {
        index = 128 + note - 35;
        if (index < 128 || index > 174)
        {
            return;
        }
    }
    else
    {
        index = mchan[channel].instrument;
    }
    ReadInstrument(index, &instr);
    patch = &instr.voices[0];

    pitch = (instr.flags & 1) ? instr.fixed_note : note + patch->base_note_offset;

    v = AllocateVoice();
    LoadOperator(&v->op[0], &patch->modulator);
    LoadOperator(&v->op[1], &patch->carrier);
    v->c0 = patch->feedback;
    v->note = note;
    v->music_channel = channel;
    v->age = ++voice_age;
    v->velocity_att = VoiceAttenuation(velocity, mchan[channel].volume);
    v->keyon = 1;
    SetFrequency(v, pitch, mchan[channel].bend);
    v->op[0].phase = v->op[1].phase = 0;
    v->op[0].prev = v->op[0].out = 0;
    if (v->op[0].state == EG_OFF) v->op[0].att16 = 511u << 16;
    if (v->op[1].state == EG_OFF) v->op[1].att16 = 511u << 16;
    v->op[0].state = EG_ATTACK;
    v->op[1].state = EG_ATTACK;
}

static void NoteOff(int channel, int note)
{
    int i;

    for (i = 0; i < NUM_VOICES; ++i)
    {
        if (voices[i].keyon && voices[i].music_channel == channel && voices[i].note == note)
        {
            KeyOff(&voices[i]);
        }
    }
}

static void AllNotesOff(int channel)
{
    int i;

    for (i = 0; i < NUM_VOICES; ++i)
    {
        if (voices[i].keyon && (channel < 0 || voices[i].music_channel == channel))
        {
            KeyOff(&voices[i]);
        }
    }
}

static void SilenceAll(void)
{
    memset(voices, 0, sizeof(voices));
}

static void ResetChannels(void)
{
    int i;

    for (i = 0; i < 16; ++i)
    {
        mchan[i].instrument = 0;
        mchan[i].volume = 100;
        mchan[i].bend = 128;
        mchan[i].last_velocity = 100;
    }
}

// ---------------------------------------------------------------- MUS player

typedef struct
{
    const uint8_t *data;
    uint32_t length;
    uint32_t start;
    uint32_t end;
} song_t;

static song_t song_store;
static const song_t *current = NULL;
static uint32_t song_pos;
static int song_delay;
static int song_playing = 0, song_looping = 0, song_paused = 0;

static boolean Mus_Init(void)
{
    int lump;

    InitTables();
    lump = W_CheckNumForName("GENMIDI");
    if (lump < 0)
    {
        return false;
    }
    genmidi = W_CacheLumpNum(lump, PU_STATIC);
    if (W_LumpLength(lump) < 8 + 175 * 36 || memcmp(genmidi, "#OPL_II#", 8) != 0)
    {
        genmidi = NULL;
        return false;
    }
    SilenceAll();
    ResetChannels();
    return true;
}

static void Mus_Shutdown(void)
{
    song_playing = 0;
    SilenceAll();
}

static void Mus_SetVolume(int volume)
{
    music_volume = volume < 0 ? 0 : (volume > 127 ? 127 : volume);
}

static void Mus_Pause(void)
{
    song_paused = 1;
    AllNotesOff(-1);
}

static void Mus_Resume(void)
{
    song_paused = 0;
}

static void *Mus_Register(void *data, int len)
{
    const uint8_t *d = data;

    if (len < 16 || memcmp(d, "MUS\x1a", 4) != 0)
    {
        return NULL;
    }
    song_store.data = d;
    song_store.length = len;
    song_store.start = d[6] | (d[7] << 8);
    song_store.end = song_store.start + (d[4] | (d[5] << 8));
    if (song_store.end > (uint32_t) len)
    {
        song_store.end = len;
    }
    return &song_store;
}

static void Mus_Unregister(void *handle)
{
    if (handle == current)
    {
        song_playing = 0;
        current = NULL;
        AllNotesOff(-1);
    }
}

static void Mus_Play(void *handle, boolean looping)
{
    if (handle == NULL)
    {
        return;
    }
    AllNotesOff(-1);
    ResetChannels();
    current = handle;
    song_pos = current->start;
    song_delay = 0;
    song_looping = looping;
    song_playing = 1;
    song_paused = 0;
}

static void Mus_Stop(void)
{
    song_playing = 0;
    AllNotesOff(-1);
}

static boolean Mus_IsPlaying(void)
{
    return song_playing;
}

static void Mus_Poll(void)
{
}

// one MUS tick (1/140 s): runs every event that is due
static void SongTick(void)
{
    int guard = 0;

    if (!song_playing || song_paused)
    {
        return;
    }
    if (song_delay > 0)
    {
        --song_delay;
    }
    while (song_delay == 0 && song_playing && guard++ < 2000)
    {
        for (;;)
        {
            uint8_t b, type, chan;
            int last;

            if (song_pos >= current->end)
            {
                b = 0x60;  // behave as if the score had ended
                type = 6;
                chan = 0;
                last = 0;
            }
            else
            {
                b = current->data[song_pos++];
                last = b & 0x80;
                type = (b >> 4) & 7;
                chan = b & 15;
            }

            switch (type)
            {
                case 0:  // release note
                    NoteOff(chan, current->data[song_pos++] & 127);
                    break;
                case 1:  // play note
                {
                    uint8_t n = current->data[song_pos++];
                    int vel = -1;

                    if (n & 0x80)
                    {
                        vel = current->data[song_pos++] & 127;
                    }
                    NoteOn(chan, n & 127, vel);
                    break;
                }
                case 2:  // pitch bend
                    mchan[chan].bend = current->data[song_pos++];
                    break;
                case 3:  // system event
                {
                    uint8_t c = current->data[song_pos++];

                    if (c == 10 || c == 11)
                    {
                        AllNotesOff(chan);
                    }
                    break;
                }
                case 4:  // controller
                {
                    uint8_t c = current->data[song_pos++];
                    uint8_t val = current->data[song_pos++];

                    if (c == 0)
                    {
                        mchan[chan].instrument = val < 128 ? val : 0;
                    }
                    else if (c == 3)
                    {
                        mchan[chan].volume = val & 127;
                    }
                    break;
                }
                case 5:  // end of measure
                    break;
                case 6:  // end of score
                    if (song_looping && current->end > current->start)
                    {
                        song_pos = current->start;
                        AllNotesOff(-1);
                    }
                    else
                    {
                        song_playing = 0;
                        AllNotesOff(-1);
                    }
                    last = 1;
                    song_delay = 1;
                    goto batch_done;
                default:
                    break;
            }

            if (last)
            {
                int delay = 0;

                for (;;)
                {
                    uint8_t d = song_pos < current->end ? current->data[song_pos++] : 0;

                    delay = (delay << 7) | (d & 127);
                    if (!(d & 0x80))
                    {
                        break;
                    }
                }
                song_delay = delay;
                break;
            }
        }
batch_done:
        break;
    }
}

music_module_t DG_music_module =
{
    sfx_devices,
    sizeof(sfx_devices) / sizeof(*sfx_devices),
    Mus_Init,
    Mus_Shutdown,
    Mus_SetVolume,
    Mus_Pause,
    Mus_Resume,
    Mus_Register,
    Mus_Unregister,
    Mus_Play,
    Mus_Stop,
    Mus_IsPlaying,
    Mus_Poll,
};

// ---------------------------------------------------------------- mixing

static uint32_t opl_acc = 0;
static uint32_t tick_acc = 0;
static int opl_last = 0;

// Loud passages (several effects plus the full band) would hard-clip, which
// sounds like static. Above the knee the signal is squeezed instead.
static int SoftLimit(int x)
{
    const int knee = 20000;

    if (x > knee)
    {
        x = knee + (x - knee) / 4;
    }
    else if (x < -knee)
    {
        x = -knee + (x + knee) / 4;
    }
    if (x > 32767) x = 32767;
    if (x < -32768) x = -32768;
    return x;
}

// Fills `frames` stereo frames of 16 bit audio at 48000 Hz.
void DG_MixAudio(int16_t *out, int frames)
{
    int f, c;

    for (f = 0; f < frames; ++f)
    {
        int left = 0, right = 0, music;

        for (c = 0; c < NUM_CHANNELS; ++c)
        {
            channel_t *ch = &channels[c];
            uint32_t index;
            int a, b, s;

            if (!ch->active)
            {
                continue;
            }
            index = ch->pos >> 16;
            if (index + 1 >= ch->length)
            {
                ch->active = 0;
                continue;
            }
            a = ch->data[index] - 128;
            b = ch->data[index + 1] - 128;
            s = (a * (int) (65536 - (ch->pos & 0xffff)) + b * (int) (ch->pos & 0xffff)) >> 16;   // -128..127
            left += (s * ch->left) * 3 / 4;
            right += (s * ch->right) * 3 / 4;
            ch->pos += ch->step;
        }

        if (tables_ready)
        {
            tick_acc += 140;
            if (tick_acc >= MIX_RATE)
            {
                tick_acc -= MIX_RATE;
                SongTick();
            }
            opl_acc += OPL_RATE;
            while (opl_acc >= MIX_RATE)
            {
                opl_acc -= MIX_RATE;
                opl_last = OplSample();
            }
            music = (opl_last * music_volume) / 200;
            left += music;
            right += music;
        }

        out[f * 2] = (int16_t) SoftLimit(left);
        out[f * 2 + 1] = (int16_t) SoftLimit(right);
    }
}
