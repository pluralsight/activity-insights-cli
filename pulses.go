package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io/ioutil"
	"log"
	"net/http"
	"path/filepath"
	"time"

	enry "github.com/go-enry/go-enry/v2"
)

var fileLanguageMap map[string]string = map[string]string{}

func getLanguage(fileName string) (string, error) {
	language := fileLanguageMap[fileName]
	if language == "" {
		fileContents, err := ioutil.ReadFile(fileName)
		if err != nil {
			return "", err
		}

		if language = enry.GetLanguage(fileName, fileContents); language == "" {
			if ext := filepath.Ext(fileName); ext == ".vsct" {
				language = "XML"
			} else {
				language = "Other"
			}
		}
		fileLanguageMap[fileName] = language
	}
	return language, nil
}

func sendPulses(pulses []pulse) {
	creds, err := fetchCredentials(credentialsFileName)
	panicOnError(err, "Error fetching creds when sending pulses")

	if creds.APIToken != "" {
		request, err := createAPIRequest(creds.APIToken, pulses)
		panicOnError(err, "Error creating API request")

		client := &http.Client{}
		resp, err := client.Do(request)
		panicOnError(err, fmt.Sprintf("Error sending the request with Bearer token: %s", creds.APIToken))

		defer resp.Body.Close()
		body, err := ioutil.ReadAll(resp.Body)
		log.Printf("Request completed with status code: %d\n", resp.StatusCode)

		if err != nil {
			log.Println("Error reading body", err)
		}

		if resp.StatusCode >= 400 {
			log.Println(string(body))
		}
	}
}

func createAPIRequest(apiToken string, pulses []pulse) (*http.Request, error) {
	url := "https://app.pluralsight.com/wsd/api/ps-time/pulse"
	reqBody, err := json.Marshal(map[string][]pulse{
		"pulses": pulses,
	})

	if err != nil {
		return nil, fmt.Errorf("Can't serialize events\n%s", err)
	}

	req, err := http.NewRequest("POST", url, bytes.NewBuffer(reqBody))
	if err != nil {
		return nil, fmt.Errorf("Error creating request with url: %s and body: %s\n%s", url, reqBody, err)
	}

	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Authorization", fmt.Sprintf("Bearer %s", apiToken))

	return req, nil
}

func buildPulses(incomingEvents []incomingEvent) []pulse {
	pulses := make([]pulse, 0)
	for _, incomingEvent := range incomingEvents {
		language, err := getLanguage(incomingEvent.FilePath)
		if err != nil {
			log.Printf("Error trying to determing the language of file: %s\n%s", incomingEvent.FilePath, err)
		} else {
			p := pulse{
				Type:                incomingEvent.EventType,
				Date:                time.Unix(0, incomingEvent.EventDate*1_000_000).Format(time.RFC3339Nano),
				ProgrammingLanguage: language,
				Editor:              incomingEvent.Editor,
			}
			pulses = append(pulses, p)
		}
	}

	return pulses
}

func processPulses() {
	payload := getPayload()

	var incomingEvents []incomingEvent
	err := json.Unmarshal(payload, &incomingEvents)
	panicOnError(err, fmt.Sprintf("Error deserializing the events from payload: %s\n", payload))

	pulses := buildPulses(incomingEvents)
	sendPulses(pulses)
}
