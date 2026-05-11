; Rainmeter actively polls the local daemon. The daemon response must keep
; these JSON fields in order: accent_color, balance_line, status_line,
; last_check, service_status_line, estimated_line.
DaemonStatusUrl=http://127.0.0.1:17654/widget-status?lang=en
DaemonCheckUrl=http://127.0.0.1:17654/check?lang=en
FallbackAccentColor=60,105,102
FallbackBalanceLine=⚠ Open the main app
FallbackStatusLine=No local data detected
LabelLastCheck=🕐 Last check
FallbackLastCheck=Not checked
LabelService=📡 DeepSeek API Status:
FallbackServiceStatusLine=⚪ Not Connected
FallbackEstimatedLine=📊 Waiting for data
RefreshText=Check
