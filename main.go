package main

import (
	"errors"
	"log"
	"os"
	"os/exec"
	"runtime"
)

func main() {
	logFile := setLogFile("ps-activity-insights.logs")
	defer logFile.Close()

	if len(os.Args) > 1 && os.Args[1] == "register" {
		registerUser()
	} else if len(os.Args) > 1 && os.Args[1] == "dashboard" {
		openBrowser("https://app.pluralsight.com/activity-insights-beta/")
	} else {
		processPulses()
	}

	fileInfo, err := logFile.Stat()
	if err == nil {
		if fileInfo.Size() > 1E5 {
			logFile.Truncate(0)
		}
	}
}

func openBrowser(url string) {
	var err error
	log.Println("Registering a user")

	switch runtime.GOOS {
	case "linux":
		err = exec.Command("xdg-open", url).Start()
	case "windows":
		err = exec.Command("cmd.exe", "/c", "start", url).Start()
	case "darwin":
		err = exec.Command("open", url).Start()
	default:
		err = errors.New("Don't recognize OS")
	}

	panicOnError(err, "Error opening browser")
}

func panicOnError(err error, msg string) {
	if err != nil {
		log.Println(msg, err)
		panic(msg)
	}
}
