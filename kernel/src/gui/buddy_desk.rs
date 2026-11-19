//! Bit on the desktop: it walks along the dock, reacts to what you do, and
//! has a very small "brain" made of four classic (tiny) machine learning
//! models:
//!
//! * a **bigram Markov chain** that writes Bit's little musings, trained on a
//!   built-in corpus plus everything you say to Bit (so it picks up your words);
//! * a **naive Bayes classifier** that guesses what you mean when you chat;
//!   your thumbs up teaches it new examples;
//! * an **epsilon-greedy bandit** that learns which tricks you like: pet Bit
//!   right after a trick and that trick gets more likely;
//! * a **first-order Markov model of your apps** that predicts what you open next.

use super::gfx::{self, Canvas, Rect};
use super::theme::Theme;
use super::ui::{id, idn, App, Ui};
use crate::graphics::buddy;
use crate::graphics::font::Font;
use crate::graphics::framebuffer::{self, Framebuffer};
use crate::input::Key;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU8, Ordering};
use spin::Mutex;

// ---------------------------------------------------------------- the brain

const CORPUS: &str = "i like pixels. i like it when you click me. the dock is warm today. \
i wonder what the stars are made of. i counted every pixel on this screen. \
maybe we should play snake later. i think the terminal is my favourite window. \
the files app keeps everything tidy. music makes my legs want to dance. \
i am a very small buddy with very big dreams. do you want to see a trick. \
i had a dream about a giant cursor. the wallpaper looks nice today. \
i am learning new words from you. integers are my favourite numbers. \
i wonder what is behind the windows. let us build something cool today. \
i like it when the windows wobble. coffee would be nice but i am made of pixels.";

const INTENTS: [(&str, &[&str]); 15] = [
    ("greet", &["hi", "hello", "hey bit", "good morning", "yo", "hello there", "hiya", "hey", "good evening"]),
    ("howareyou", &["how are you", "how do you feel", "are you ok", "whats up", "how is it going", "how are things", "you good"]),
    ("joke", &["tell me a joke", "make me laugh", "say something funny", "joke please", "know any jokes", "funny"]),
    ("time", &["what time is it", "what is the time", "what day is it", "date today", "current time", "which day"]),
    ("name", &["what is your name", "who are you", "what are you", "your name", "are you a robot"]),
    ("thanks", &["thanks", "thank you", "nice one", "cool thanks", "cheers", "appreciate it"]),
    ("bye", &["bye", "goodbye", "see you", "good night", "later", "see ya"]),
    ("compliment", &["you are cute", "i like you", "good job", "you are awesome", "love you bit", "well done", "you are cool"]),
    ("insult", &["you are dumb", "stupid", "i hate you", "you are useless", "bad bit", "you suck"]),
    ("bored", &["i am bored", "nothing to do", "so bored", "entertain me", "what should i do", "boring"]),
    ("open", &["open files", "launch terminal", "start snake", "open the browser", "play doom", "run calculator", "open settings", "open media", "start pong", "launch notes"]),
    ("help", &["help", "what can you do", "how does this work", "show me around", "tips", "how do i"]),
    ("trick", &["dance", "do a trick", "show me a trick", "do something", "spin", "jump", "wave"]),
    ("feel", &["i am sad", "i feel bad", "bad day", "i am tired", "i am happy", "great day", "i feel good"]),
    ("weather", &["how is the weather", "is it raining", "is it sunny", "weather today"]),
];

const REPLIES: [(&str, &[&str]); 15] = [
    ("greet", &["hi there!", "hello, friend!", "hey hey! nice to see you.", "oh hi! i was just counting pixels."]),
    ("howareyou", &["i'm great, thanks for asking!", "a bit sleepy, but happy.", "all my pixels are in order!", "fantastic. the dock is comfy."]),
    ("joke", &["why did the pixel break up? it needed space.", "i'd tell you a udp joke, but you might not get it.", "there are 10 kinds of buddies: ones that get binary and ones that don't.", "my favourite exercise? the dock stretch."]),
    ("time", &["@time"]),
    ("name", &["i'm Bit! your little desktop buddy.", "Bit. one bit of personality, many bits of pixels."]),
    ("thanks", &["anytime!", "happy to help!", "you're welcome!"]),
    ("bye", &["bye! i'll keep the dock warm.", "see you soon!", "good night, sleep tight."]),
    ("compliment", &["aww, you're making me blush!", "you're pretty great yourself.", "i like you too!"]),
    ("insult", &["ouch. i'm only a few pixels big, you know.", "that hurt my feelings.exe", "i'll pretend i didn't hear that."]),
    ("bored", &["want to play snake? just say 'open snake'.", "try Mission Control with F3, it's fun!", "let's make a ZeroScript game together!", "@muse"]),
    ("open", &["@open"]),
    ("help", &["try Super+Space to search, F3 for Mission Control, F4 for Launchpad. right-click works almost everywhere!", "drag windows to the screen edges to snap them. double-click my face to chat!"]),
    ("trick", &["@trick"]),
    ("feel", &["i hear you. want a hug from a pixel?", "whatever it is, we can get through it together.", "that's great to hear!"]),
    ("weather", &["inside the computer it's always 37 degrees and cloudless.", "no clouds here, just windows."]),
];

const TRICKS: [&str; 10] = ["dance", "spin", "hop", "juggle", "wave", "cheer", "flex", "bubbles", "balloon", "celebrate"];

struct Brain {
    /// bigram counts: word -> (next word -> count)
    chain: BTreeMap<String, BTreeMap<String, u32>>,
    /// naive Bayes: intent -> word counts, and examples per intent
    words: Vec<BTreeMap<String, u32>>,
    totals: Vec<u32>,
    docs: Vec<u32>,
    /// reply template weights (successes, tries)
    reply_w: Vec<Vec<(u32, u32)>>,
    /// trick bandit: (successes, tries)
    tricks: [(u32, u32); TRICKS.len()],
    pending_trick: Option<(usize, u64)>,
    /// app transitions: (from, to) -> count
    apps: BTreeMap<(String, String), u32>,
    last_app: Option<String>,
    rng: u32,
    learned_examples: Vec<(usize, String)>,
    last_intent: Option<(usize, usize, String)>,
}

static BRAIN: Mutex<Option<Brain>> = Mutex::new(None);
static CHATTY: AtomicU8 = AtomicU8::new(2);
static QUESTION: Mutex<Option<String>> = Mutex::new(None);

fn tokens(s: &str) -> Vec<String> {
    s.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '\'')
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect()
}

impl Brain {
    fn new() -> Brain {
        let mut b = Brain {
            chain: BTreeMap::new(),
            words: (0..INTENTS.len()).map(|_| BTreeMap::new()).collect(),
            totals: alloc::vec![0; INTENTS.len()],
            docs: alloc::vec![0; INTENTS.len()],
            reply_w: REPLIES.iter().map(|(_, r)| alloc::vec![(1, 2); r.len()]).collect(),
            tricks: [(1, 2); TRICKS.len()],
            pending_trick: None,
            apps: BTreeMap::new(),
            last_app: None,
            rng: 0x1234_5678 ^ crate::time::uptime_ms() as u32 | 1,
            learned_examples: Vec::new(),
            last_intent: None,
        };
        for sentence in CORPUS.split('.') {
            b.learn_text(sentence);
        }
        for (k, (_, examples)) in INTENTS.iter().enumerate() {
            for e in examples.iter() {
                b.train(k, e);
            }
        }
        b.load();
        b
    }

    fn rand(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }

    /// Adds a sentence to the Markov chain.
    fn learn_text(&mut self, s: &str) {
        let t = tokens(s);
        if t.len() < 2 {
            return;
        }
        let mut prev = String::from("^");
        for w in t.iter().chain(core::iter::once(&String::from("$"))) {
            *self.chain.entry(prev.clone()).or_default().entry(w.clone()).or_insert(0) += 1;
            prev = w.clone();
        }
    }

    /// Writes a sentence by walking the chain.
    fn muse(&mut self) -> String {
        let mut out: Vec<String> = Vec::new();
        let mut cur = String::from("^");
        for _ in 0..14 {
            let roll = self.rand();
            let Some(next) = self.chain.get(&cur) else { break };
            let total: u32 = next.values().sum();
            let mut pick = roll % total.max(1);
            let mut chosen = String::from("$");
            for (w, c) in next.iter() {
                if pick < *c {
                    chosen = w.clone();
                    break;
                }
                pick -= c;
            }
            if chosen == "$" {
                break;
            }
            out.push(chosen.clone());
            cur = chosen;
        }
        if out.is_empty() {
            return String::from("...");
        }
        let mut s = out.join(" ");
        s.push('.');
        s
    }

    fn train(&mut self, intent: usize, text: &str) {
        for w in tokens(text) {
            *self.words[intent].entry(w).or_insert(0) += 1;
            self.totals[intent] += 1;
        }
        self.docs[intent] += 1;
    }

    /// Naive Bayes: the most likely intent and how sure (0..=100).
    fn classify(&self, text: &str) -> (usize, u32) {
        let t = tokens(text);
        let vocab: usize = self.words.iter().map(|m| m.len()).sum::<usize>().max(1);
        let all_docs: u32 = self.docs.iter().sum::<u32>().max(1);
        let mut scores = Vec::with_capacity(INTENTS.len());
        for k in 0..INTENTS.len() {
            let mut s = libm::logf(self.docs[k].max(1) as f32 / all_docs as f32);
            let denom = (self.totals[k] as usize + vocab) as f32;
            for w in &t {
                let c = *self.words[k].get(w).unwrap_or(&0) as f32;
                s += libm::logf((c + 1.0) / denom);
            }
            scores.push(s);
        }
        let mut best = 0;
        for k in 1..scores.len() {
            if scores[k] > scores[best] {
                best = k;
            }
        }
        // how far ahead of the runner-up, as a rough confidence
        let mut second = f32::MIN;
        for (k, s) in scores.iter().enumerate() {
            if k != best && *s > second {
                second = *s;
            }
        }
        let gap = scores[best] - second;
        let known = t.iter().any(|w| self.words.iter().any(|m| m.contains_key(w)));
        let conf = if !known { 0 } else { ((gap * 25.0) as i32).clamp(0, 100) as u32 };
        (best, conf)
    }

    /// Picks a reply template, favouring the ones you liked.
    fn pick_reply(&mut self, intent: usize) -> usize {
        let w = self.reply_w[intent].clone();
        let total: u32 = w.iter().map(|(s, t)| s * 1000 / t.max(&1)).sum();
        let mut r = self.rand() % total.max(1);
        for (k, (s, t)) in w.iter().enumerate() {
            let v = s * 1000 / t.max(&1);
            if r < v {
                return k;
            }
            r -= v;
        }
        0
    }

    fn feedback(&mut self, good: bool) {
        if let Some((intent, reply, text)) = self.last_intent.take() {
            let w = &mut self.reply_w[intent][reply];
            w.1 += 1;
            if good {
                w.0 += 1;
                // a good answer means the guess was right: learn the sentence
                self.train(intent, &text);
                self.learned_examples.push((intent, text));
                if self.learned_examples.len() > 12 {
                    self.learned_examples.remove(0);
                }
            }
            self.save();
        }
    }

    fn choose_trick(&mut self) -> usize {
        // explore 20% of the time, otherwise the trick with the best success rate
        if self.rand() % 100 < 20 {
            return (self.rand() as usize) % TRICKS.len();
        }
        let mut best = 0;
        for k in 1..TRICKS.len() {
            let (s, t) = self.tricks[k];
            let (bs, bt) = self.tricks[best];
            if s * bt > bs * t {
                best = k;
            }
        }
        best
    }

    fn settle_trick(&mut self, now: u64, petted: bool) {
        if let Some((k, at)) = self.pending_trick {
            if petted && now < at + 6000 {
                self.tricks[k].0 += 1;
                self.tricks[k].1 += 1;
                self.pending_trick = None;
                self.save();
            } else if now >= at + 6000 {
                self.tricks[k].1 += 1;
                self.pending_trick = None;
            }
        }
    }

    fn predict_next(&self, app: &str) -> Option<(String, u32)> {
        self.apps.iter().filter(|((a, _), _)| a == app).max_by_key(|(_, c)| **c).map(|((_, b), c)| (b.clone(), *c))
    }

    fn save(&self) {
        let mut t = String::new();
        for (s, n) in self.tricks.iter() {
            t.push_str(&alloc::format!("{}/{},", s, n));
        }
        crate::settings::set("bit_tricks", &t);
        let mut a = String::new();
        let mut top: Vec<(&(String, String), &u32)> = self.apps.iter().collect();
        top.sort_by(|x, y| y.1.cmp(x.1));
        for ((x, y), c) in top.into_iter().take(10) {
            a.push_str(&alloc::format!("{}>{}:{},", x, y, c));
        }
        crate::settings::set("bit_apps", &a);
        let mut e = String::new();
        for (k, text) in self.learned_examples.iter() {
            let short: String = text.chars().filter(|c| *c != '|' && *c != ';').take(40).collect();
            e.push_str(&alloc::format!("{}|{};", k, short));
        }
        crate::settings::set("bit_learned", &e);
    }

    fn load(&mut self) {
        if let Some(t) = crate::settings::get("bit_tricks") {
            for (k, part) in t.split(',').filter(|p| !p.is_empty()).enumerate().take(TRICKS.len()) {
                if let Some((s, n)) = part.split_once('/') {
                    self.tricks[k] = (s.parse().unwrap_or(1), n.parse().unwrap_or(2));
                }
            }
        }
        if let Some(a) = crate::settings::get("bit_apps") {
            for part in a.split(',').filter(|p| !p.is_empty()) {
                if let Some((pair, c)) = part.rsplit_once(':') {
                    if let Some((x, y)) = pair.split_once('>') {
                        self.apps.insert((String::from(x), String::from(y)), c.parse().unwrap_or(1));
                    }
                }
            }
        }
        if let Some(e) = crate::settings::get("bit_learned") {
            for part in e.split(';').filter(|p| !p.is_empty()) {
                if let Some((k, text)) = part.split_once('|') {
                    if let Ok(k) = k.parse::<usize>() {
                        if k < INTENTS.len() {
                            self.train(k, text);
                            self.learned_examples.push((k, String::from(text)));
                        }
                    }
                }
            }
        }
    }
}

fn with_brain<R>(f: impl FnOnce(&mut Brain) -> R) -> R {
    let mut g = BRAIN.lock();
    if g.is_none() {
        *g = Some(Brain::new());
    }
    f(g.as_mut().unwrap())
}

pub fn init() {
    if let Some(v) = crate::settings::get("bit_chatty") {
        CHATTY.store(v.parse().unwrap_or(2).min(3), Ordering::Relaxed);
    }
    with_brain(|_| ());
}

pub fn chattiness() -> u8 {
    CHATTY.load(Ordering::Relaxed)
}

pub fn set_chattiness(v: u8) {
    CHATTY.store(v.min(3), Ordering::Relaxed);
    crate::settings::set("bit_chatty", &alloc::format!("{}", v.min(3)));
    buddy::set_chatty(v.min(3));
}

pub fn greet() {
    let name = crate::settings::get_or("name", "friend");
    let hour = super::desktop::read_rtc().0;
    let part = if hour < 12 { "good morning" } else if hour < 18 { "good afternoon" } else { "good evening" };
    super::request(super::Request::BuddySay(alloc::format!("{}, {}!", part, name)));
}

pub fn queue_question(q: &str) {
    *QUESTION.lock() = Some(String::from(q));
}

pub fn on_app_opened(name: &str) {
    let note = with_brain(|b| {
        let mut note = None;
        if let Some(prev) = b.last_app.clone() {
            if prev != name {
                *b.apps.entry((prev, String::from(name))).or_insert(0) += 1;
            }
        }
        if let Some((next, c)) = b.predict_next(name) {
            if c >= 2 && b.rand() % 3 == 0 {
                note = Some(alloc::format!("{}! you usually open {} next.", name, next));
            }
        }
        b.last_app = Some(String::from(name));
        if b.rand() % 8 == 0 {
            b.save();
        }
        note
    });
    let text = note.or_else(|| {
        let lines = [("Terminal", "hacker mode!"), ("Snake", "go get those apples!"), ("DOOM", "rip and tear!"), ("Files", "tidying up?"), ("Browser", "surfing the web!"), ("Media", "ooh, pictures!"), ("Notes", "writing something?")];
        lines.iter().find(|(n, _)| *n == name).map(|(_, t)| String::from(*t))
    });
    if let Some(t) = text {
        if chattiness() >= 1 {
            super::request(super::Request::BuddySay(t));
        }
    }
}

pub fn learned_summary() -> Vec<String> {
    with_brain(|b| {
        let mut out = Vec::new();
        let mut best = 0;
        for k in 1..TRICKS.len() {
            let (s, t) = b.tricks[k];
            let (bs, bt) = b.tricks[best];
            if s * bt > bs * t {
                best = k;
            }
        }
        let (s, t) = b.tricks[best];
        out.push(alloc::format!("Favourite trick (by your petting): {} ({} of {} times)", TRICKS[best], s.saturating_sub(1), t.saturating_sub(2)));
        let apps: u32 = b.apps.values().sum();
        out.push(alloc::format!("App habits remembered: {} switches", apps));
        if let Some(((x, y), c)) = b.apps.iter().max_by_key(|(_, c)| **c) {
            out.push(alloc::format!("Most common: {} then {} ({} times)", x, y, c));
        }
        out.push(alloc::format!("Sentences learned from you: {}", b.learned_examples.len()));
        out.push(alloc::format!("Words in Bit's vocabulary: {}", b.chain.len()));
        out
    })
}

pub fn forget() {
    for k in ["bit_tricks", "bit_apps", "bit_learned"] {
        crate::settings::set(k, "");
    }
    *BRAIN.lock() = None;
    with_brain(|_| ());
}

/// Answers something said to Bit. Returns the reply.
pub fn answer(text: &str) -> String {
    // arithmetic first
    let cleaned: String = text.chars().filter(|c| c.is_ascii_digit() || "+-*/%() ".contains(*c)).collect();
    if let Some(v) = super::desktop::calc(&cleaned) {
        if cleaned.trim().len() >= 3 {
            return alloc::format!("that's {}!", v);
        }
    }
    let (intent, conf) = with_brain(|b| {
        b.learn_text(text);
        b.classify(text)
    });
    if conf < 8 {
        let m = with_brain(|b| b.muse());
        with_brain(|b| b.last_intent = None);
        return alloc::format!("hmm, i'm not sure what you mean. {}", m);
    }
    let reply = with_brain(|b| {
        let r = b.pick_reply(intent);
        b.last_intent = Some((intent, r, String::from(text)));
        r
    });
    let template = REPLIES[intent].1[reply];
    match template {
        "@time" => {
            let (h, m, _, d, mo, y) = super::desktop::read_rtc();
            alloc::format!("it's {:02}:{:02} on {}-{:02}-{:02}.", h, m, y, mo, d)
        }
        "@muse" => with_brain(|b| b.muse()),
        "@trick" => {
            let k = with_brain(|b| b.choose_trick());
            buddy::perform(TRICKS[k], None);
            with_brain(|b| b.pending_trick = Some((k, crate::time::uptime_ms())));
            alloc::format!("watch this: {}!", TRICKS[k])
        }
        "@open" => {
            let lower = text.to_ascii_lowercase();
            let found = super::apps::APPS.iter().find(|a| lower.contains(&a.name.to_ascii_lowercase()));
            match found {
                Some(a) => {
                    super::request(super::Request::Open(a.id, None));
                    alloc::format!("opening {} for you!", a.name)
                }
                None => String::from("which app? try 'open files' or 'open snake'."),
            }
        }
        t => String::from(t),
    }
}

// ---------------------------------------------------------------- on the dock

pub struct DeskBuddy {
    bubble: Option<(String, u64)>,
    last_tick: u64,
    next_idea: u64,
    hearts: Vec<(i32, i32, u64)>,
}

const SCALE: usize = 2;

impl DeskBuddy {
    pub fn new() -> DeskBuddy {
        DeskBuddy { bubble: None, last_tick: 0, next_idea: crate::time::uptime_ms() + 20_000, hearts: Vec::new() }
    }

    pub fn say(&mut self, text: &str) {
        self.bubble = Some((String::from(text), crate::time::uptime_ms()));
    }

    /// Advances Bit; returns true if it should be redrawn.
    pub fn tick(&mut self, now: u64) -> bool {
        with_brain(|b| b.settle_trick(now, false));
        if now >= self.next_idea {
            let level = chattiness() as u64;
            self.next_idea = now + 25_000 + (4 - level.min(3)) * 15_000;
            if level > 0 {
                let roll = with_brain(|b| b.rand() % 3);
                if roll == 0 {
                    let k = with_brain(|b| b.choose_trick());
                    buddy::perform(TRICKS[k], None);
                    with_brain(|b| b.pending_trick = Some((k, now)));
                } else if roll == 1 {
                    let m = with_brain(|b| b.muse());
                    self.say(&m);
                }
            }
        }
        if let Some((_, at)) = &self.bubble {
            if now > at + 5000 {
                self.bubble = None;
                return true;
            }
        }
        self.hearts.retain(|(_, _, t)| now < t + 1200);
        if now >= self.last_tick + 90 {
            self.last_tick = now;
            return true;
        }
        false
    }

    pub fn pet(&mut self) {
        let now = crate::time::uptime_ms();
        let had_trick = with_brain(|b| b.pending_trick.is_some());
        with_brain(|b| b.settle_trick(now, true));
        buddy::react(buddy::Event::Success);
        let lines = ["hehe, that tickles!", "yay!", "i like you too!", "purr... wait, i'm not a cat.", "more pets please!"];
        let k = with_brain(|b| b.rand() as usize % lines.len());
        self.say(if had_trick { "you liked that one? i'll remember!" } else { lines[k] });
        self.hearts.push((0, 0, now));
        crate::audio::effect(crate::audio::Effect::Click);
    }

    fn lane(&self, bar: Rect) -> (i32, i32) {
        (bar.0 + 12, (bar.2 - 24).max(80))
    }

    /// Where Bit is on screen.
    pub fn rect(&self, bar: Rect) -> Rect {
        let (lx, lw) = self.lane(bar);
        let x = buddy::screen_x(lx as usize, lw as usize, SCALE) as i32;
        let (cw, ch) = buddy::SPRITE;
        (x, bar.1 - (ch * SCALE) as i32 - 4, (cw * SCALE) as i32, (ch * SCALE) as i32 + 4)
    }

    pub fn draw(&self, cv: &mut Canvas, bar: Rect, th: &Theme, f: &Font) {
        let (lx, lw) = self.lane(bar);
        let (_, ch) = buddy::SPRITE;
        let h = (ch * SCALE) as i32 + 60;
        let key = 0xFF00FF;
        let Some(mut fb) = framebuffer::with_screen(|s| Framebuffer::offscreen32(s, lw as usize, h as usize)) else { return };
        fb.pixels32().fill(key);
        buddy::draw(&mut fb, 0, lw as usize, h as usize, SCALE);
        let floor = bar.1 + 2;
        let top = floor - h;
        let px = fb.pixels32_ref();
        // a soft shadow under Bit, then Bit
        let r = self.rect(bar);
        cv.round_a(r.0 + 2, floor - 3, r.2 - 4, 4, 2, 0x000000, 60);
        for y in 0..h {
            for x in 0..lw {
                let c = px[(y * lw + x) as usize];
                if c != key {
                    cv.put(lx + x, top + y, c, 255);
                }
            }
        }
        // hearts float up after a pet
        let now = crate::time::uptime_ms();
        for (k, (_, _, t)) in self.hearts.iter().enumerate() {
            let age = (now - t) as i32;
            let hx = r.0 + r.2 / 2 + (k as i32 % 3 - 1) * 10 + super::icons::sin_deg(age / 3) * 6 / 1000;
            let hy = r.1 - age / 20;
            let a = (255 - age * 255 / 1200).max(0) as u32;
            cv.circle_a(hx - 3, hy, 4, 0xFF4D6D, a);
            cv.circle_a(hx + 3, hy, 4, 0xFF4D6D, a);
            for i in 0..6 {
                cv.rect_a(hx - 6 + i, hy + 1 + i, 13 - 2 * i, 1, 0xFF4D6D, a);
            }
        }
        // the speech bubble: Bit's own words, or what the old engine is thinking
        let text = self.bubble.as_ref().map(|(t, _)| t.clone()).or_else(buddy::thought);
        if let Some(text) = text {
            let lines = gfx::wrap(f, &text, 240);
            let bw = lines.iter().map(|l| gfx::text_width(f, l)).max().unwrap_or(0) + 24;
            let bh = lines.len() as i32 * (f.height() as i32 + 3) + 14;
            let bx = (r.0 + r.2 / 2 - bw / 2).max(4);
            let by = r.1 - bh - 14;
            cv.shadow(bx, by + 3, bw, bh, 12, 12, 80);
            cv.round(bx, by, bw, bh, 12, th.menu);
            cv.round_outline(bx, by, bw, bh, 12, th.menu_border, 160);
            // the tail
            for i in 0..8 {
                cv.rect(r.0 + r.2 / 2 - 8 + i, by + bh - 1 + i, 16 - 2 * i, 1, th.menu);
            }
            for (k, l) in lines.iter().enumerate() {
                cv.text(bx + 12, by + 7 + k as i32 * (f.height() as i32 + 3), l, f, th.text);
            }
        }
    }
}

// ---------------------------------------------------------------- the chat window

struct Msg {
    from_bit: bool,
    text: String,
    rated: bool,
}

pub struct Chat {
    msgs: Vec<Msg>,
    input: String,
    started: u64,
}

impl Chat {
    pub fn new() -> Chat {
        let name = crate::settings::get_or("name", "friend");
        let mut c = Chat { msgs: Vec::new(), input: String::new(), started: crate::time::uptime_ms() };
        c.msgs.push(Msg { from_bit: true, text: alloc::format!("hi {}! talk to me. ask for a joke, the time, a trick, or say 'open snake'.", name), rated: true });
        c
    }

    fn send(&mut self, text: String) {
        if text.trim().is_empty() {
            return;
        }
        let reply = answer(&text);
        self.msgs.push(Msg { from_bit: false, text, rated: true });
        let teachable = with_brain(|b| b.last_intent.is_some());
        self.msgs.push(Msg { from_bit: true, text: reply.clone(), rated: !teachable });
        super::request(super::Request::BuddySay(reply));
    }
}

impl App for Chat {
    fn title(&self) -> String {
        String::from("Bit")
    }

    fn frame(&mut self, ui: &mut Ui) {
        if let Some(q) = QUESTION.lock().take() {
            self.send(q);
        }
        let th = ui.th;
        let (w, h) = (ui.w, ui.h);
        ui.fill(if th.dark { 0x1C1D21 } else { 0xF2F2F5 });
        // header with Bit's face
        ui.cv.rect(0, 0, w, 64, th.window);
        ui.cv.hline(0, 63, w, th.separator, 255);
        super::icons::draw(&mut ui.cv, super::icons::Icon::Buddy, 14, 10, 44);
        ui.text_bold(68, 14, "Bit", th.text);
        let s = buddy::status();
        ui.muted(68, 34, &alloc::format!("feeling {}", s.mood));
        // messages, newest at the bottom
        let f = ui.font;
        let maxw = w * 2 / 3;
        let mut layout: Vec<(Vec<String>, i32)> = Vec::new();
        let mut total = 16;
        for m in &self.msgs {
            let lines = gfx::wrap(&f, &m.text, maxw - 24);
            let bh = lines.len() as i32 * (f.height() as i32 + 3) + 16 + if m.from_bit && !m.rated { 30 } else { 0 };
            total += bh + 10;
            layout.push((lines, bh));
        }
        let area = (0, 64, w, h - 64 - 60);
        let sid = id("chatscroll");
        let max = (total - area.3).max(0);
        if ui.scroll_of(sid) == 0 || self.started + 100 > ui.now {
            ui.scroll_to(sid, max);
        }
        let off = ui.scroll_begin(sid, area, total);
        let mut y = area.1 + 12 - off;
        let mut rate: Option<(usize, bool)> = None;
        for (k, (m, (lines, bh))) in self.msgs.iter().zip(layout.iter()).enumerate() {
            let bw = lines.iter().map(|l| gfx::text_width(&f, l)).max().unwrap_or(0) + 24;
            let bw = bw.max(if m.from_bit && !m.rated { 130 } else { 0 });
            let x = if m.from_bit { 14 } else { w - bw - 14 };
            let (bg, fg) = if m.from_bit { (if th.dark { 0x34363C } else { 0xFFFFFF }, th.text) } else { (th.accent, th.on_accent) };
            ui.cv.shadow(x, y + 1, bw, *bh, 14, 4, 40);
            ui.cv.round(x, y, bw, *bh, 14, bg);
            for (i, l) in lines.iter().enumerate() {
                ui.cv.text(x + 12, y + 8 + i as i32 * (f.height() as i32 + 3), l, &f, fg);
            }
            if m.from_bit && !m.rated {
                let by = y + bh - 32;
                if ui.button((x + 10, by, 52, 24), "good") {
                    rate = Some((k, true));
                }
                if ui.button((x + 68, by, 52, 24), "meh") {
                    rate = Some((k, false));
                }
            }
            y += bh + 10;
        }
        ui.scroll_end(sid, area, total);
        if let Some((k, good)) = rate {
            with_brain(|b| b.feedback(good));
            self.msgs[k].rated = true;
            self.msgs.push(Msg { from_bit: true, text: String::from(if good { "yay! i learned something." } else { "noted. i'll try something else next time." }), rated: true });
            ui.scroll_to(sid, i32::MAX / 2);
        }
        // input
        ui.cv.rect(0, h - 60, w, 60, th.window);
        ui.cv.hline(0, h - 60, w, th.separator, 255);
        ui.st.focus = id("chatin");
        let enter = ui.field(id("chatin"), (14, h - 46, w - 112, 32), &mut self.input, "Message Bit");
        let send = ui.primary((w - 88, h - 46, 74, 32), "Send");
        if enter || send {
            let t = core::mem::take(&mut self.input);
            self.send(t);
            ui.scroll_to(sid, i32::MAX / 2);
        }
        let _ = (idn("x", 0), Key::Home, 0.to_string());
    }
}
