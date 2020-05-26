package main

type credentials struct {
	APIToken string `yaml:"api_token"`
}

type incomingEvent struct {
	FilePath  string `json:"filePath"`
	EventType string `json:"eventType"`
	EventDate int64  `json:"eventDate"`
	Editor    string `json:"editor"`
}

type pulse struct {
	Type                string `json:"type"`
	Date                string `json:"date"`
	ProgrammingLanguage string `json:"programmingLanguage"`
	Editor              string `json:"editor"`
}
