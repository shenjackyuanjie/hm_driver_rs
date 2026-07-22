/// OpenHarmony 常用按键码。
///
/// 未列入枚举的平台扩展码仍可通过 `HmDriver::press_key(u32)` 发送。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum KeyCode {
    /// 未知按键。
    Unknown = -1,
    /// 功能键。
    Fn = 0,
    /// 主页键。
    Home = 1,
    /// 返回键。
    Back = 2,
    /// 媒体播放/暂停。
    MediaPlayPause = 10,
    /// 媒体停止。
    MediaStop = 11,
    /// 媒体下一曲。
    MediaNext = 12,
    /// 媒体上一曲。
    MediaPrevious = 13,
    /// 媒体快退。
    MediaRewind = 14,
    /// 媒体快进。
    MediaFastForward = 15,
    /// 音量加。
    VolumeUp = 16,
    /// 音量减。
    VolumeDown = 17,
    /// 电源键。
    Power = 18,
    /// 相机键。
    Camera = 19,
    /// 音量静音。
    VolumeMute = 22,
    /// 静音。
    Mute = 23,
    /// 亮度加。
    BrightnessUp = 40,
    /// 亮度减。
    BrightnessDown = 41,
    /// 数字键 0。
    Num0 = 2000,
    /// 数字键 1。
    Num1 = 2001,
    /// 数字键 2。
    Num2 = 2002,
    /// 数字键 3。
    Num3 = 2003,
    /// 数字键 4。
    Num4 = 2004,
    /// 数字键 5。
    Num5 = 2005,
    /// 数字键 6。
    Num6 = 2006,
    /// 数字键 7。
    Num7 = 2007,
    /// 数字键 8。
    Num8 = 2008,
    /// 数字键 9。
    Num9 = 2009,
    /// 星号键。
    Star = 2010,
    /// 井号键。
    Pound = 2011,
    /// 方向键上。
    DpadUp = 2012,
    /// 方向键下。
    DpadDown = 2013,
    /// 方向键左。
    DpadLeft = 2014,
    /// 方向键右。
    DpadRight = 2015,
    /// 方向键确认。
    DpadCenter = 2016,
    /// 字母键 A。
    A = 2017,
    /// 字母键 B。
    B = 2018,
    /// 字母键 C。
    C = 2019,
    /// 字母键 D。
    D = 2020,
    /// 字母键 E。
    E = 2021,
    /// 字母键 F。
    F = 2022,
    /// 字母键 G。
    G = 2023,
    /// 字母键 H。
    H = 2024,
    /// 字母键 I。
    I = 2025,
    /// 字母键 J。
    J = 2026,
    /// 字母键 K。
    K = 2027,
    /// 字母键 L。
    L = 2028,
    /// 字母键 M。
    M = 2029,
    /// 字母键 N。
    N = 2030,
    /// 字母键 O。
    O = 2031,
    /// 字母键 P。
    P = 2032,
    /// 字母键 Q。
    Q = 2033,
    /// 字母键 R。
    R = 2034,
    /// 字母键 S。
    S = 2035,
    /// 字母键 T。
    T = 2036,
    /// 字母键 U。
    U = 2037,
    /// 字母键 V。
    V = 2038,
    /// 字母键 W。
    W = 2039,
    /// 字母键 X。
    X = 2040,
    /// 字母键 Y。
    Y = 2041,
    /// 字母键 Z。
    Z = 2042,
    /// 逗号键。
    Comma = 2043,
    /// 句号键。
    Period = 2044,
    /// 左 Alt 键。
    AltLeft = 2045,
    /// 右 Alt 键。
    AltRight = 2046,
    /// 左 Shift 键。
    ShiftLeft = 2047,
    /// 右 Shift 键。
    ShiftRight = 2048,
    /// Tab 键。
    Tab = 2049,
    /// 空格键。
    Space = 2050,
    /// 符号键。
    Sym = 2051,
    /// 资源管理器键。
    Explorer = 2052,
    /// 邮件键。
    Envelope = 2053,
    /// 回车键。
    Enter = 2054,
    /// 退格删除键。
    Delete = 2055,
    /// 反引号键。
    Grave = 2056,
    /// 减号键。
    Minus = 2057,
    /// 等号键。
    Equals = 2058,
    /// 左中括号键。
    LeftBracket = 2059,
    /// 右中括号键。
    RightBracket = 2060,
    /// 反斜杠键。
    Backslash = 2061,
    /// 分号键。
    Semicolon = 2062,
    /// 单引号键。
    Apostrophe = 2063,
    /// 斜杠键。
    Slash = 2064,
    /// At 键。
    At = 2065,
    /// 加号键。
    Plus = 2066,
    /// 菜单键。
    Menu = 2067,
    /// 上翻页键。
    PageUp = 2068,
    /// 下翻页键。
    PageDown = 2069,
    /// Escape 键。
    Escape = 2070,
    /// 向前删除键。
    ForwardDelete = 2071,
    /// 左 Ctrl 键。
    CtrlLeft = 2072,
    /// 右 Ctrl 键。
    CtrlRight = 2073,
    /// Caps Lock 键。
    CapsLock = 2074,
    /// Scroll Lock 键。
    ScrollLock = 2075,
    /// 左 Meta 键。
    MetaLeft = 2076,
    /// 右 Meta 键。
    MetaRight = 2077,
    /// 功能键。
    Function = 2078,
    /// SysRq 键。
    SysRq = 2079,
    /// Break 键。
    Break = 2080,
    /// 移动光标到行首。
    MoveHome = 2081,
    /// 移动光标到行尾。
    MoveEnd = 2082,
    /// 插入键。
    Insert = 2083,
    /// 前进键。
    Forward = 2084,
    /// 媒体播放。
    MediaPlay = 2085,
    /// 媒体暂停。
    MediaPause = 2086,
    /// 媒体关闭。
    MediaClose = 2087,
    /// 媒体弹出。
    MediaEject = 2088,
    /// 媒体录制。
    MediaRecord = 2089,
    /// 功能键 F1。
    F1 = 2090,
    /// 功能键 F2。
    F2 = 2091,
    /// 功能键 F3。
    F3 = 2092,
    /// 功能键 F4。
    F4 = 2093,
    /// 功能键 F5。
    F5 = 2094,
    /// 功能键 F6。
    F6 = 2095,
    /// 功能键 F7。
    F7 = 2096,
    /// 功能键 F8。
    F8 = 2097,
    /// 功能键 F9。
    F9 = 2098,
    /// 功能键 F10。
    F10 = 2099,
    /// 功能键 F11。
    F11 = 2100,
    /// 功能键 F12。
    F12 = 2101,
    /// Num Lock 键。
    NumLock = 2102,
    /// 数字小键盘 0。
    Numpad0 = 2103,
    /// 数字小键盘 1。
    Numpad1 = 2104,
    /// 数字小键盘 2。
    Numpad2 = 2105,
    /// 数字小键盘 3。
    Numpad3 = 2106,
    /// 数字小键盘 4。
    Numpad4 = 2107,
    /// 数字小键盘 5。
    Numpad5 = 2108,
    /// 数字小键盘 6。
    Numpad6 = 2109,
    /// 数字小键盘 7。
    Numpad7 = 2110,
    /// 数字小键盘 8。
    Numpad8 = 2111,
    /// 数字小键盘 9。
    Numpad9 = 2112,
    /// 数字小键盘除号。
    NumpadDivide = 2113,
    /// 数字小键盘乘号。
    NumpadMultiply = 2114,
    /// 数字小键盘减号。
    NumpadSubtract = 2115,
    /// 数字小键盘加号。
    NumpadAdd = 2116,
    /// 数字小键盘小数点。
    NumpadDot = 2117,
    /// 数字小键盘逗号。
    NumpadComma = 2118,
    /// 数字小键盘回车。
    NumpadEnter = 2119,
    /// 数字小键盘等号。
    NumpadEquals = 2120,
    /// 数字小键盘左括号。
    NumpadLeftParen = 2121,
    /// 数字小键盘右括号。
    NumpadRightParen = 2122,
    /// 虚拟多任务键。
    VirtualMultitask = 2210,
    /// 休眠键。
    Sleep = 2600,
    /// 全角半角切换键。
    ZenkakuHankaku = 2601,
    /// 假名 `nd` 键。
    Nd = 2602,
    /// `Ro` 键（日文罗马字输入）。
    Ro = 2603,
    /// 片假名键。
    Katakana = 2604,
    /// 平假名键。
    Hiragana = 2605,
    /// 变换键（日文输入法）。
    Henkan = 2606,
    /// 片假名/平假名切换键。
    KatakanaHiragana = 2607,
    /// 无变换键（日文输入法）。
    Muhenkan = 2608,
    /// 换行键。
    Linefeed = 2609,
    /// 宏键。
    Macro = 2610,
    /// 数字小键盘正负号。
    NumpadPlusMinus = 2611,
    /// 缩放键。
    Scale = 2612,
    /// 韩语切换键。
    Hanguel = 2613,
    /// 韩语汉字键。
    Hanja = 2614,
    /// 日元符号键。
    Yen = 2615,
    /// 停止键。
    Stop = 2616,
    /// 重复键。
    Again = 2617,
    /// 属性键。
    Props = 2618,
    /// 撤销键。
    Undo = 2619,
    /// 复制键。
    Copy = 2620,
    /// 打开键。
    Open = 2621,
    /// 粘贴键。
    Paste = 2622,
    /// 查找键。
    Find = 2623,
    /// 剪切键。
    Cut = 2624,
    /// 帮助键。
    Help = 2625,
    /// 计算器键。
    Calc = 2626,
    /// 文件键。
    File = 2627,
    /// 书签键。
    Bookmarks = 2628,
    /// 下一个。
    Next = 2629,
    /// 播放/暂停切换。
    PlayPause = 2630,
    /// 上一个。
    Previous = 2631,
    /// 停止 CD。
    StopCd = 2632,
    /// 配置键。
    Config = 2634,
    /// 刷新键。
    Refresh = 2635,
    /// 退出键。
    Exit = 2636,
    /// 编辑键。
    Edit = 2637,
    /// 上滚。
    ScrollUp = 2638,
    /// 下滚。
    ScrollDown = 2639,
    /// 新建。
    New = 2640,
    /// 重做键。
    Redo = 2641,
    /// 关闭键。
    Close = 2642,
    /// 播放键。
    Play = 2643,
    /// 低音增强。
    BassBoost = 2644,
    /// 打印键。
    Print = 2645,
    /// 聊天键。
    Chat = 2646,
    /// 财务键。
    Finance = 2647,
    /// 取消键。
    Cancel = 2648,
    /// 键盘背光开关。
    KeyboardIlluminationToggle = 2649,
    /// 键盘背光减。
    KeyboardIlluminationDown = 2650,
    /// 键盘背光加。
    KeyboardIlluminationUp = 2651,
    /// 发送键。
    Send = 2652,
    /// 回复键。
    Reply = 2653,
    /// 转发邮件键。
    ForwardMail = 2654,
    /// 保存键。
    Save = 2655,
    /// 文档键。
    Documents = 2656,
    /// 视频下一段。
    VideoNext = 2657,
    /// 视频上一段。
    VideoPrevious = 2658,
    /// 亮度循环切换。
    BrightnessCycle = 2659,
    /// 亮度归零。
    BrightnessZero = 2660,
    /// 关闭显示。
    DisplayOff = 2661,
    /// 杂项按钮。
    ButtonMisc = 2662,
    /// 跳转键。
    Goto = 2663,
    /// 信息键。
    Info = 2664,
    /// 节目键。
    Program = 2665,
    /// PVR 键。
    Pvr = 2666,
    /// 字幕键。
    Subtitle = 2667,
    /// 全屏键。
    FullScreen = 2668,
    /// 键盘键。
    Keyboard = 2669,
    /// 宽高比切换。
    AspectRatio = 2670,
    /// PC 模式键。
    Pc = 2671,
    /// 电视键。
    Tv = 2672,
    /// 电视键 2。
    Tv2 = 2673,
    /// 录像机键。
    Vcr = 2674,
    /// 录像机键 2。
    Vcr2 = 2675,
    /// 卫星键。
    Sat = 2676,
    /// CD 键。
    Cd = 2677,
    /// 磁带键。
    Tape = 2678,
    /// 调谐器键。
    Tuner = 2679,
    /// 播放器键。
    Player = 2680,
    /// DVD 键。
    Dvd = 2681,
    /// 音频键。
    Audio = 2682,
    /// 视频键。
    Video = 2683,
    /// 备忘录键。
    Memo = 2684,
    /// 日历键。
    Calendar = 2685,
    /// 红色功能键。
    Red = 2686,
    /// 绿色功能键。
    Green = 2687,
    /// 黄色功能键。
    Yellow = 2688,
    /// 蓝色功能键。
    Blue = 2689,
    /// 频道加。
    ChannelUp = 2690,
    /// 频道减。
    ChannelDown = 2691,
    /// 上一个频道。
    Last = 2692,
    /// 重启键。
    Restart = 2693,
    /// 慢放键。
    Slow = 2694,
    /// 随机播放键。
    Shuffle = 2695,
    /// 视频电话。
    VideoPhone = 2696,
    /// 游戏键。
    Games = 2697,
    /// 放大。
    ZoomIn = 2698,
    /// 缩小。
    ZoomOut = 2699,
    /// 重置缩放。
    ZoomReset = 2700,
    /// 文字处理器键。
    WordProcessor = 2701,
    /// 编辑器键。
    Editor = 2702,
    /// 电子表格键。
    Spreadsheet = 2703,
    /// 图形编辑器键。
    GraphicsEditor = 2704,
    /// 演示文稿键。
    Presentation = 2705,
    /// 数据库键。
    Database = 2706,
    /// 新闻键。
    News = 2707,
    /// 语音信箱键。
    Voicemail = 2708,
    /// 通讯录键。
    AddressBook = 2709,
    /// 即时通讯键。
    Messenger = 2710,
    /// 亮度开关。
    BrightnessToggle = 2711,
    /// 拼写检查键。
    Spellcheck = 2712,
    /// 咖啡键（唤醒屏幕）。
    Coffee = 2713,
    /// 媒体重复播放。
    MediaRepeat = 2714,
    /// 图片键。
    Images = 2715,
    /// 配置按钮键。
    ButtonConfig = 2716,
    /// 任务管理器键。
    TaskManager = 2717,
    /// 日志键。
    Journal = 2718,
    /// 控制面板键。
    ControlPanel = 2719,
    /// 应用选择键。
    AppSelect = 2720,
    /// 屏幕保护键。
    ScreenSaver = 2721,
    /// 助手键。
    Assistant = 2722,
    /// 下一个键盘布局。
    KeyboardLayoutNext = 2723,
    /// 最小亮度。
    BrightnessMin = 2724,
    /// 最大亮度。
    BrightnessMax = 2725,
    /// 键盘输入辅助上一个。
    KeyboardInputAssistPrevious = 2726,
    /// 键盘输入辅助下一个。
    KeyboardInputAssistNext = 2727,
    /// 键盘输入辅助上一组。
    KeyboardInputAssistPreviousGroup = 2728,
    /// 键盘输入辅助下一组。
    KeyboardInputAssistNextGroup = 2729,
    /// 键盘输入辅助接受。
    KeyboardInputAssistAccept = 2730,
    /// 键盘输入辅助取消。
    KeyboardInputAssistCancel = 2731,
    /// 前置键。
    Front = 2800,
    /// 设置键。
    Setup = 2801,
    /// 唤醒键。
    WakeUp = 2802,
    /// 发送文件键。
    SendFile = 2803,
    /// 删除文件键。
    DeleteFile = 2804,
    /// 传输键。
    Transfer = 2805,
    /// 节目键 1。
    Program1 = 2806,
    /// 节目键 2。
    Program2 = 2807,
    /// MS-DOS 键。
    MsDos = 2808,
    /// 屏幕锁定键。
    ScreenLock = 2809,
    /// 方向旋转显示。
    DirectionRotateDisplay = 2810,
    /// 循环切换窗口。
    CycleWindows = 2811,
    /// 电脑键。
    Computer = 2812,
    /// 弹出/关闭 CD 仓。
    EjectCloseCd = 2813,
    /// ISO 键。
    Iso = 2814,
    /// 移动键。
    Move = 2815,
    /// 功能键 F13。
    F13 = 2816,
    /// 功能键 F14。
    F14 = 2817,
    /// 功能键 F15。
    F15 = 2818,
    /// 功能键 F16。
    F16 = 2819,
    /// 功能键 F17。
    F17 = 2820,
    /// 功能键 F18。
    F18 = 2821,
    /// 功能键 F19。
    F19 = 2822,
    /// 功能键 F20。
    F20 = 2823,
    /// 功能键 F21。
    F21 = 2824,
    /// 功能键 F22。
    F22 = 2825,
    /// 功能键 F23。
    F23 = 2826,
    /// 功能键 F24。
    F24 = 2827,
    /// 节目键 3。
    Program3 = 2828,
    /// 节目键 4。
    Program4 = 2829,
    /// 仪表盘键。
    Dashboard = 2830,
    /// 挂起键。
    Suspend = 2831,
    /// HP 键。
    Hp = 2832,
    /// 音效键。
    Sound = 2833,
    /// 问号键。
    Question = 2834,
    /// 连接键。
    Connect = 2836,
    /// 运动键。
    Sport = 2837,
    /// 购物键。
    Shop = 2838,
    /// 擦除键。
    Alterase = 2839,
    /// 切换视频模式。
    SwitchVideoMode = 2841,
    /// 电池键。
    Battery = 2842,
    /// 蓝牙键。
    Bluetooth = 2843,
    /// 无线局域网键。
    Wlan = 2844,
    /// UWB 键。
    Uwb = 2845,
    /// WWAN/WiMax 键。
    WwanWimax = 2846,
    /// 射频开关。
    RfKill = 2847,
    /// 频道键。
    Channel = 3001,
    /// 通用按钮 0。
    Button0 = 3100,
    /// 通用按钮 1。
    Button1 = 3101,
    /// 通用按钮 2。
    Button2 = 3102,
    /// 通用按钮 3。
    Button3 = 3103,
    /// 通用按钮 4。
    Button4 = 3104,
    /// 通用按钮 5。
    Button5 = 3105,
    /// 通用按钮 6。
    Button6 = 3106,
    /// 通用按钮 7。
    Button7 = 3107,
    /// 通用按钮 8。
    Button8 = 3108,
    /// 通用按钮 9。
    Button9 = 3109,
}

impl KeyCode {
    /// 返回当前键码的原始整数值。
    pub const fn value(self) -> i32 {
        self as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_full_range_and_unknown_code() {
        assert_eq!(KeyCode::Unknown.value(), -1);
        assert_eq!(KeyCode::Home.value(), 1);
        assert_eq!(KeyCode::Wlan.value(), 2844);
        assert_eq!(KeyCode::Button9.value(), 3109);
    }
}
