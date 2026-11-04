'use strict';
/* ============ FR-SYS-006：中英文界面切换 ============
   策略：静态界面文案打 data-i18n="键"（文本）或 data-i18n-title="键"（提示），
   由 applyI18n() 在加载与语言切换时重写；动态文案随语言分支输出。
   未标记文案保持中文，属于渐进式覆盖，不会出现空白界面。 */
const I18N = {
  zh: {
    'nav.model': '模型', 'nav.persona': '人格', 'nav.pet': '宠物与交互', 'nav.memory': '记忆',
    'nav.data': '会话与数据', 'nav.privacy': '隐私', 'nav.about': '关于',
    'set.title': '设置 — 桌面 AI 宠物', 'common.save': '保存', 'common.delete': '删除', 'common.close': '关闭',
    'common.cancel': '取消', 'common.copy': '复制', 'common.add': '添加', 'common.none': '暂无数据',
    'bubble.placeholder': '说点什么…（记住：… / N 分钟后提醒我…）', 'bubble.send': '发送',
    'bubble.stop': '停止生成', 'bubble.new': '新会话', 'bubble.settings': '设置',
    'bubble.switch': '快速切换模型 / 人格', 'bubble.hint': 'Enter 发送 · Shift+Enter 换行 · Esc 关闭',
    'bubble.thinking': '思考中…', 'bubble.retry': '重试', 'bubble.switchModel': '切换模型',
    'bubble.openSettings': '打开设置', 'bubble.speak': '朗读这条回复',
    'bubble.ttsUnsupported': '当前环境不支持语音朗读', 'bubble.ttsFailed': '朗读失败：',
    'pet.chat': '打开对话', 'pet.newSession': '新会话',
    'pet.settings': '设置', 'pet.dnd': '免打扰（睡眠）', 'pet.resetPos': '复位宠物位置', 'pet.skin': '皮肤',
    'about.lang': '界面语言', 'about.zh': '中文', 'about.en': 'English',
  },
  en: {
    'nav.model': 'Models', 'nav.persona': 'Personas', 'nav.pet': 'Pet & Input', 'nav.memory': 'Memory',
    'nav.data': 'Sessions', 'nav.privacy': 'Privacy', 'nav.about': 'About',
    'set.title': 'Settings — DeskPet', 'common.save': 'Save', 'common.delete': 'Delete', 'common.close': 'Close',
    'common.cancel': 'Cancel', 'common.copy': 'Copy', 'common.add': 'Add', 'common.none': 'Nothing here yet',
    'bubble.placeholder': 'Say something… ("remember: …" / "remind me in 20 min")', 'bubble.send': 'Send',
    'bubble.stop': 'Stop', 'bubble.new': 'New session', 'bubble.settings': 'Settings',
    'bubble.switch': 'Switch model / persona', 'bubble.hint': 'Enter to send · Shift+Enter for newline · Esc to close',
    'bubble.thinking': 'Thinking…', 'bubble.retry': 'Retry', 'bubble.switchModel': 'Switch model',
    'bubble.openSettings': 'Open settings', 'bubble.speak': 'Read this reply aloud',
    'bubble.ttsUnsupported': 'Speech synthesis is not available in this environment', 'bubble.ttsFailed': 'Speech failed: ',
    'pet.chat': 'Open chat', 'pet.newSession': 'New session',
    'pet.settings': 'Settings', 'pet.dnd': 'Do not disturb', 'pet.resetPos': 'Reset position', 'pet.skin': 'Skin',
    'about.lang': 'Language', 'about.zh': '中文', 'about.en': 'English',
  },
};
function t(key) {
  const lang = (typeof S !== 'undefined' && S && S.settings && S.settings.lang) || 'zh';
  return (I18N[lang] && I18N[lang][key]) || I18N.zh[key] || key;
}
function applyI18n() {
  const lang = (typeof S !== 'undefined' && S && S.settings && S.settings.lang) || 'zh';
  document.documentElement.lang = lang === 'en' ? 'en' : 'zh-CN';
  document.querySelectorAll('[data-i18n]').forEach(n => { n.textContent = t(n.dataset.i18n); });
  document.querySelectorAll('[data-i18n-title]').forEach(n => { n.title = t(n.dataset.i18nTitle); });
  document.querySelectorAll('[data-i18n-ph]').forEach(n => { n.placeholder = t(n.dataset.i18nPh); });
}
