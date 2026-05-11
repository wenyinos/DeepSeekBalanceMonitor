; Rainmeter actively polls the local daemon. The daemon response must keep
; these JSON fields in order: accent_color, balance_line, status_line,
; last_check, service_status_line, estimated_line.
DaemonStatusUrl=http://127.0.0.1:17654/widget-status?lang=zh
DaemonCheckUrl=http://127.0.0.1:17654/check?lang=zh
FallbackAccentColor=60,105,102
FallbackBalanceLine=⚠ 请打开原进程
FallbackStatusLine=未识别到本地数据
LabelLastCheck=🕐 上次查询
FallbackLastCheck=尚未查询
LabelService=📡 DeepSeek API 服务状态：
FallbackServiceStatusLine=⚪ 未连接
FallbackEstimatedLine=📊 等待数据
RefreshText=查询
