//! Машина состояний анимированного значка: общий ход для значков программ семьи (окно, трей, «О программе»).
//!
//! Значок программы — картинка, а не фигуры egui: кадр считает растеризатор программы, а **когда** и **что**
//! рисовать — эта машина. Код чистый, без egui; время — секунды `f64` от любого начала (часы программы).
//!
//! Кирпичики (у каждого значка — свой набор, см. `docs/MOTION.md`, «Машина состояний значка»):
//!
//! | Тип | Что это | Пример |
//! |---|---|---|
//! | [`Phase`] | состояние с переходами: текущее, прежнее, сколько прошло | связь Amber, режим центра, маршрут Morok |
//! | [`Pulse`] | одноразовое событие, повтор — заново | новое сообщение |
//! | [`Hold`] | пока включено — цикл с периодом | звенит звонок |
//! | [`Badge`] | счётчик: 0 → n — всплеск, дальше без повторов | непрочитанное |
//! | [`Follow`] | число догоняет цель плавно | доля хода задачи |
//! | [`Script`] | состояния по очереди с длительностями | «О программе»: подключение → готово |
//! | [`window`] | доля 0..1 внутри отрезка эпизода | блик с задержкой, кольцо |
//! | [`Scene`] + [`Presenter`] | значок и его показ: файл в покое, кадры ≤ N в секунду, кадр на смену | значок окна и трея |
//!
//! Правила те же, что у всего движения: кадры просятся только пока что-то движется; «меньше движения» — картинка
//! меняется только со сменой состояния ([`Scene::key`]).

/// «Давно»: эпизоды, начатые тогда, уже кончились.
pub const LONG_AGO: f64 = -1.0e6;

/// Доля `0..1` внутри отрезка эпизода `[delay; delay + secs)`; вне его — `None`.
/// `t` — время от начала эпизода.
pub fn window(t: f32, delay: f32, secs: f32) -> Option<f32> {
    let w = (t - delay) / secs;
    (w > 0.0 && w < 1.0).then_some(w)
}

/// Время в секундах между `now` и `then`, как `f32` (большие часы не теряют точность до вычитания).
fn since(now: f64, then: f64) -> f32 {
    (now - then) as f32
}

// ─── Состояние ──────────────────────────────────────────────────────────────

/// Состояние с переходами: текущее, прежнее и момент смены. Переход играет сцена — по паре
/// «откуда → куда» и времени [`Phase::age`].
#[derive(Debug, Clone, PartialEq)]
pub struct Phase<S> {
    state: S,
    prev: Option<S>,
    since: f64,
}

impl<S: Copy + PartialEq> Phase<S> {
    /// Начальное состояние без предыстории: перехода нет, значок неподвижен.
    pub fn new(state: S) -> Self {
        Self { state, prev: None, since: LONG_AGO }
    }

    /// Сменить состояние; то же — ничего не меняет. Возвращает, сменилось ли.
    pub fn set(&mut self, state: S, now: f64) -> bool {
        if state == self.state {
            return false;
        }
        self.prev = Some(self.state);
        self.state = state;
        self.since = now;
        true
    }

    pub fn state(&self) -> S {
        self.state
    }

    /// Откуда пришли; `None` — состояние с самого начала.
    pub fn prev(&self) -> Option<S> {
        self.prev
    }

    /// Секунд с последней смены.
    pub fn age(&self, now: f64) -> f32 {
        since(now, self.since)
    }

    /// Сейчас `to`, пришли из `from`.
    pub fn is(&self, from: S, to: S) -> bool {
        self.prev == Some(from) && self.state == to
    }
}

// ─── События ────────────────────────────────────────────────────────────────

/// Одноразовое событие: играет с момента [`Pulse::fire`]; ещё одно подряд — начинается заново.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pulse {
    at: f64,
}

impl Default for Pulse {
    fn default() -> Self {
        Self { at: LONG_AGO }
    }
}

impl Pulse {
    pub fn fire(&mut self, now: f64) {
        self.at = now;
    }

    /// Секунд с последнего события.
    pub fn age(&self, now: f64) -> f32 {
        since(now, self.at)
    }

    /// Играет ли эпизод длиной `secs`.
    pub fn playing(&self, now: f64, secs: f32) -> bool {
        (0.0..secs).contains(&self.age(now))
    }

    /// Доля внутри отрезка `[delay; delay + secs)` от события — см. [`window`].
    pub fn window(&self, now: f64, delay: f32, secs: f32) -> Option<f32> {
        window(self.age(now), delay, secs)
    }
}

/// Удержание: пока включено — цикл с периодом (звенит звонок, идёт задача).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hold {
    on: bool,
    start: f64,
}

impl Default for Hold {
    fn default() -> Self {
        Self { on: false, start: LONG_AGO }
    }
}

impl Hold {
    /// Включить или выключить; цикл считается с момента включения.
    pub fn set(&mut self, on: bool, now: f64) {
        if on && !self.on {
            self.start = now;
        }
        self.on = on;
    }

    pub fn on(&self) -> bool {
        self.on
    }

    /// Секунд от начала текущего цикла длиной `period`; выключено — `None`.
    pub fn cycle(&self, now: f64, period: f64) -> Option<f32> {
        self.on.then(|| (now - self.start).rem_euclid(period) as f32)
    }
}

/// Счётчик со всплеском: 0 → n — событие (точка вырастает), n → m — без повтора, → 0 — гаснет сразу.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Badge {
    count: usize,
    pop: Pulse,
}

impl Badge {
    /// Новый счёт. `pop = false` (например, окно на виду) — точка появляется без всплеска.
    pub fn set(&mut self, count: usize, now: f64, pop: bool) {
        if self.count == 0 && count > 0 {
            self.pop = Pulse::default();
            if pop {
                self.pop.fire(now);
            }
        }
        self.count = count;
    }

    pub fn count(&self) -> usize {
        self.count
    }

    pub fn shown(&self) -> bool {
        self.count > 0
    }

    /// Всплеск появления: время от него (для `0..длина`); без точки — `None`.
    pub fn pop(&self) -> Option<&Pulse> {
        self.shown().then_some(&self.pop)
    }
}

/// Число догоняет цель плавно: экспонента с постоянной времени `tau` секунд. Новая цель — догоняет её;
/// [`Follow::reset`] — встать сразу (новая задача начинает полоску с нуля).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Follow {
    value: f32,
    target: f32,
    tau: f32,
    at: Option<f64>,
}

impl Follow {
    pub fn new(value: f32, tau: f32) -> Self {
        Self { value, target: value, tau, at: None }
    }

    pub fn reset(&mut self, value: f32) {
        self.value = value;
        self.target = value;
    }

    pub fn set(&mut self, target: f32) {
        self.target = target;
    }

    /// Шаг к моменту `now`; `still` — сразу к цели.
    pub fn step(&mut self, now: f64, still: bool) -> f32 {
        let dt = self.at.map_or(0.0, |at| since(now, at).max(0.0));
        self.at = Some(now);
        self.value = if still || self.tau <= 0.0 {
            self.target
        } else {
            self.target + (self.value - self.target) * (-dt / self.tau).exp()
        };
        if (self.value - self.target).abs() < 1e-3 {
            self.value = self.target;
        }
        self.value
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    /// Ещё догоняет — кадры нужны.
    pub fn moving(&self) -> bool {
        self.value != self.target
    }
}

/// Сценарий: состояния по очереди со своими длительностями, последнее остаётся. «О программе» Amber:
/// `[(Conn, 1.1)]`, затем `On`; центра: `[(Busy, 1.2)]`, затем `Idle`.
#[derive(Debug, Clone, PartialEq)]
pub struct Script<S> {
    steps: Vec<(S, f64)>,
    last: S,
    start: f64,
}

impl<S: Copy + PartialEq> Script<S> {
    pub fn new(steps: &[(S, f64)], last: S, start: f64) -> Self {
        Self { steps: steps.to_vec(), last, start }
    }

    /// Какое состояние в момент `now` и с какого момента оно идёт.
    pub fn at(&self, now: f64) -> (S, f64) {
        let mut from = self.start;
        for (state, secs) in &self.steps {
            if now < from + secs {
                return (*state, from);
            }
            from += secs;
        }
        (self.last, from)
    }

    /// Довести `phase` до момента `now`: каждая смена — в свой момент, а не в момент вызова.
    pub fn drive(&self, phase: &mut Phase<S>, now: f64) {
        let mut from = self.start;
        for (state, secs) in &self.steps {
            if now < from {
                return;
            }
            phase.set(*state, from);
            from += secs;
        }
        if now >= from {
            phase.set(self.last, from);
        }
    }

    /// Все шаги позади — дальше последнее состояние.
    pub fn finished(&self, now: f64) -> bool {
        now >= self.start + self.steps.iter().map(|(_, s)| s).sum::<f64>()
    }
}

// ─── Сцена и показ ──────────────────────────────────────────────────────────

/// Значок программы: свои кирпичики внутри, кадр — числа для своего растеризатора.
pub trait Scene {
    /// Числа одного кадра.
    type Frame;
    /// Что видно, когда ничего не движется (состояние, есть ли точка, доля хода с шагом 5 %…).
    type Key: PartialEq + Clone;

    /// Кадр в момент `now`; `still` — «меньше движения».
    fn frame(&self, now: f64, still: bool) -> Self::Frame;
    /// Что-то движется — нужны кадры. При `still` — обычно никогда.
    fn wants_frames(&self, now: f64, still: bool) -> bool;
    /// Значок выглядит как файл программы: можно вернуть файл и ничего не рисовать.
    fn at_rest(&self, now: f64, still: bool) -> bool;
    /// Неподвижный вид: сменился — кадр рисуется один раз.
    fn key(&self) -> Self::Key;
}

/// Что сделать со значком на этом шаге.
#[derive(Debug, Clone, PartialEq)]
pub enum Show<F> {
    /// Ничего: показанное верно (или кадр ещё рано — потолок кадров).
    Keep,
    /// Вернуть значок из файла программы.
    File,
    /// Поставить этот кадр.
    Frame(F),
}

/// Показ значка: в покое — файл, пока движется — кадры не чаще `fps` в секунду, неподвижный не-покой —
/// один кадр на каждую смену [`Scene::key`]. Один `Presenter` на одно место (окно, трей).
#[derive(Debug, Clone)]
pub struct Presenter<K> {
    every: f64,
    last: f64,
    live: bool,
    key: Option<K>,
}

impl<K: PartialEq + Clone> Presenter<K> {
    /// `fps` — потолок кадров в секунду (значок окна и трея — 10).
    pub fn new(fps: f64) -> Self {
        Self { every: 1.0 / fps, last: LONG_AGO, live: false, key: None }
    }

    /// Шаг: что показать сейчас.
    pub fn step<S: Scene<Key = K>>(&mut self, scene: &S, now: f64, still: bool) -> Show<S::Frame> {
        if scene.at_rest(now, still) {
            self.key = None;
            return if std::mem::take(&mut self.live) { Show::File } else { Show::Keep };
        }
        if scene.wants_frames(now, still) {
            self.key = None;
            if now - self.last < self.every {
                return Show::Keep;
            }
            self.last = now;
            self.live = true;
            return Show::Frame(scene.frame(now, still));
        }
        let key = scene.key();
        if self.live && self.key.as_ref() == Some(&key) {
            return Show::Keep;
        }
        self.live = true;
        self.key = Some(key);
        Show::Frame(scene.frame(now, still))
    }

    /// Через сколько секунд нужен следующий шаг; `None` — ничего не движется, кадров не просить.
    pub fn next_in<S: Scene<Key = K>>(&self, scene: &S, now: f64, still: bool) -> Option<f64> {
        scene.wants_frames(now, still).then(|| (self.every - (now - self.last)).max(0.0))
    }

    /// Показанное больше не верно (новое место показа, сменилась текстура): следующий шаг рисует заново.
    pub fn invalidate(&mut self) {
        self.key = None;
        self.last = LONG_AGO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Link {
        Conn,
        On,
        Down,
    }

    /// Игрушечный значок: точки при подключении, эпизод 1 с при связи, остывание 0,4 с, сообщение 0,5 с.
    struct Toy {
        link: Phase<Link>,
        message: Pulse,
    }

    impl Scene for Toy {
        type Frame = (Link, bool);
        type Key = Link;
        fn frame(&self, now: f64, still: bool) -> (Link, bool) {
            (self.link.state(), self.wants_frames(now, still))
        }
        fn wants_frames(&self, now: f64, still: bool) -> bool {
            if still {
                return false;
            }
            let t = self.link.age(now);
            let episode = match self.link.state() {
                Link::Conn => true,
                Link::On => self.link.prev().is_some() && t < 1.0,
                Link::Down => t < 0.4,
            };
            episode || self.message.playing(now, 0.5)
        }
        fn at_rest(&self, now: f64, still: bool) -> bool {
            self.link.state() == Link::On && !self.wants_frames(now, still)
        }
        fn key(&self) -> Link {
            self.link.state()
        }
    }

    fn toy() -> Toy {
        Toy { link: Phase::new(Link::On), message: Pulse::default() }
    }

    #[test]
    fn a_phase_remembers_where_it_came_from() {
        let mut phase = Phase::new(Link::On);
        assert_eq!((phase.prev(), phase.age(0.0) > 1000.0), (None, true), "no history: no transition plays");
        assert!(phase.set(Link::Conn, 1.0));
        assert!(!phase.set(Link::Conn, 2.0), "the same state changes nothing");
        assert_eq!(phase.age(1.5), 0.5);
        phase.set(Link::On, 3.0);
        assert!(phase.is(Link::Conn, Link::On) && !phase.is(Link::Down, Link::On));
    }

    #[test]
    fn a_pulse_plays_once_and_restarts() {
        let mut pulse = Pulse::default();
        assert!(!pulse.playing(0.0, 1.0));
        pulse.fire(1.0);
        assert!(pulse.playing(1.2, 0.5) && !pulse.playing(1.5, 0.5));
        assert!((pulse.window(1.3, 0.1, 0.4).unwrap() - 0.5).abs() < 1e-4);
        assert_eq!(pulse.window(1.05, 0.1, 0.4), None, "before the delay");
        pulse.fire(1.4);
        assert!(pulse.playing(1.6, 0.5), "again: from the start");
    }

    #[test]
    fn a_hold_cycles_while_on_and_stops() {
        let mut hold = Hold::default();
        assert_eq!(hold.cycle(1.0, 2.0), None);
        hold.set(true, 1.0);
        hold.set(true, 1.5);
        assert_eq!(hold.cycle(3.5, 2.0), Some(0.5), "counted from switching on, not from repeats");
        hold.set(false, 4.0);
        assert_eq!(hold.cycle(4.1, 2.0), None);
    }

    #[test]
    fn a_badge_pops_only_from_zero() {
        let mut badge = Badge::default();
        assert!(badge.pop().is_none());
        badge.set(1, 1.0, true);
        assert!(badge.pop().unwrap().playing(1.2, 0.7));
        badge.set(5, 2.0, true);
        assert!(!badge.pop().unwrap().playing(2.1, 0.7), "n → m does not pop again");
        badge.set(0, 3.0, true);
        assert!(!badge.shown() && badge.pop().is_none(), "all read: gone at once");
        badge.set(2, 4.0, false);
        assert!(badge.shown() && !badge.pop().unwrap().playing(4.0, 0.7), "no pop asked: appears quietly");
    }

    #[test]
    fn follow_catches_up_and_still_jumps() {
        let mut fill = Follow::new(0.0, 0.12);
        fill.step(0.0, false);
        fill.set(1.0);
        let early = fill.step(0.12, false);
        assert!((early - (1.0 - (-1.0f32).exp())).abs() < 1e-4, "one time constant: 63 %");
        assert!(fill.moving());
        fill.step(2.0, false);
        assert_eq!(fill.value(), 1.0);
        assert!(!fill.moving(), "close enough snaps: no endless frames");
        fill.set(0.3);
        assert_eq!(fill.step(2.01, true), 0.3, "reduced motion: at once");
        fill.reset(0.0);
        assert_eq!(fill.value(), 0.0);
    }

    #[test]
    fn a_script_drives_a_phase_at_its_own_moments() {
        let script = Script::new(&[(Link::Conn, 1.1)], Link::On, 10.0);
        assert_eq!(script.at(10.5), (Link::Conn, 10.0));
        assert_eq!(script.at(12.0), (Link::On, 11.1));
        let mut phase = Phase::new(Link::On);
        script.drive(&mut phase, 10.0);
        assert!(phase.is(Link::On, Link::Conn));
        // Шаг пришёл поздно — смена всё равно в свой момент.
        script.drive(&mut phase, 11.5);
        assert!(phase.is(Link::Conn, Link::On));
        assert!((phase.age(11.5) - 0.4).abs() < 1e-4);
        assert!(script.finished(11.1) && !script.finished(11.0));
    }

    #[test]
    fn the_presenter_shows_the_file_at_rest_and_caps_frames() {
        let mut toy = toy();
        let mut show = Presenter::new(10.0);
        assert_eq!(show.step(&toy, 0.0, false), Show::Keep, "at rest from the start: the file is already there");
        assert_eq!(show.next_in(&toy, 0.0, false), None, "nothing moves: no frames asked");
        toy.link.set(Link::Conn, 1.0);
        assert!(matches!(show.step(&toy, 1.0, false), Show::Frame(_)));
        assert_eq!(show.step(&toy, 1.05, false), Show::Keep, "no more than 10 per second");
        assert!((show.next_in(&toy, 1.05, false).unwrap() - 0.05).abs() < 1e-9);
        assert!(matches!(show.step(&toy, 1.1, false), Show::Frame(_)));
        toy.link.set(Link::On, 2.0);
        assert!(matches!(show.step(&toy, 2.5, false), Show::Frame(_)), "the episode plays");
        assert_eq!(show.step(&toy, 3.0, false), Show::File, "the episode is over: back to the file");
        assert_eq!(show.step(&toy, 3.5, false), Show::Keep);
    }

    #[test]
    fn a_still_state_draws_one_frame_per_change() {
        let mut toy = toy();
        let mut show = Presenter::new(10.0);
        toy.link.set(Link::Down, 0.0);
        // Остывает — кадры, остыл — последний кадр «остывший» и тишина.
        assert!(matches!(show.step(&toy, 0.1, false), Show::Frame(_)));
        assert!(matches!(show.step(&toy, 0.5, false), Show::Frame(_)), "the settled look once");
        assert_eq!(show.step(&toy, 0.6, false), Show::Keep);
        assert_eq!(show.next_in(&toy, 0.6, false), None);
        // «Меньше движения»: подключение — один кадр, без потока кадров.
        toy.link.set(Link::Conn, 1.0);
        assert!(matches!(show.step(&toy, 1.0, true), Show::Frame(_)));
        assert_eq!(show.step(&toy, 1.5, true), Show::Keep);
        toy.link.set(Link::On, 2.0);
        assert_eq!(show.step(&toy, 2.0, true), Show::File);
        // Сообщение поверх покоя — кадры, кончилось — файл.
        toy.message.fire(3.0);
        assert!(matches!(show.step(&toy, 3.0, false), Show::Frame(_)));
        assert_eq!(show.step(&toy, 3.6, false), Show::File);
        show.invalidate();
        toy.link.set(Link::Down, 4.0);
        assert!(matches!(show.step(&toy, 4.0, false), Show::Frame(_)));
    }

    #[test]
    fn window_is_open_only_inside() {
        assert!((window(0.3, 0.1, 0.4).unwrap() - 0.5).abs() < 1e-4);
        assert_eq!(window(0.1, 0.1, 0.4), None);
        assert_eq!(window(0.5, 0.1, 0.4), None);
    }
}
