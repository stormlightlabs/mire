package echo

import (
	"io"
	"os"

	"github.com/charmbracelet/log"
	"github.com/muesli/termenv"
)

// NewLogger creates the diagnostic logger used by the process.
//
// Logs default to [os.Stderr], intentionally keeping [os.Stdout] available
// for command results.
func NewLogger(output io.Writer) *log.Logger {
	if output == nil {
		output = os.Stderr
	}

	logger := log.NewWithOptions(output, log.Options{
		Level:           log.InfoLevel,
		Formatter:       log.TextFormatter,
		ReportTimestamp: false,
		ReportCaller:    false,
	})
	logger.SetStyles(loggerStyles())
	if ColorDisabled() {
		logger.SetColorProfile(termenv.Ascii)
	}
	return logger
}

func loggerStyles() *log.Styles {
	styles := log.DefaultStyles()
	styles.Timestamp = styleMuted()
	styles.Caller = styleMuted()
	styles.Prefix = styleAccent()
	styles.Message = styleMessage()
	styles.Key = styleMuted()
	styles.Value = styleValue()
	styles.Separator = styleMuted()
	styles.Levels[log.DebugLevel] = styleMuted()
	styles.Levels[log.InfoLevel] = styleValue()
	styles.Levels[log.WarnLevel] = makeStyle(colorYellow, true)
	styles.Levels[log.ErrorLevel] = makeStyle(colorPink, true)
	styles.Levels[log.FatalLevel] = makeStyle(colorPink, true)
	return styles
}
