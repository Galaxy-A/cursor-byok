package bridge

import "github.com/pkg/browser"

const footerAuthorHomeURL = "https://space.bilibili.com/311706663/upload/video"

var footerAuthorInfo = FooterAuthorInfo{
	ButtonText:        "作者 leookun",
	DialogTitle:       "作者寄语",
	DialogContent:     "本软件是纯免费软件，如果你被收费，那大概率就是被骗了。\n欢迎点击访问作者主页 https://space.bilibili.com/311706663/upload/video\n查看更多更新动态、使用分享和后续内容。\n\n本项目是非官方社区项目，与 Anysphere 或 Cursor 官方不存在隶属、赞助或背书关系。Cursor 名称及相关商标归其权利人所有。",
	DialogConfirmText: "访问主页",
	DialogCancelText:  "关闭",
}

// FooterAuthorInfo 定义首页底部作者入口的展示信息。
type FooterAuthorInfo struct {
	ButtonText        string `json:"buttonText"`
	DialogTitle       string `json:"dialogTitle"`
	DialogContent     string `json:"dialogContent"`
	DialogConfirmText string `json:"dialogConfirmText"`
	DialogCancelText  string `json:"dialogCancelText"`
}

// GetFooterAuthorInfo 返回首页底部作者入口的展示信息。
func (s *WindowService) GetFooterAuthorInfo() FooterAuthorInfo {
	return footerAuthorInfo
}

// OpenFooterAuthorHome 打开作者主页。
func (s *WindowService) OpenFooterAuthorHome() error {
	return browser.OpenURL(footerAuthorHomeURL)
}
