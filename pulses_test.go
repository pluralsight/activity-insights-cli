package main

import (
	"fmt"
	"os"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
)

func TestGetLanguage(t *testing.T) {
	t.Run("gets the language of the file using enry", func(t *testing.T) {
		// Setup
		fileName := "testing.js"
		file, err := os.Create(fileName)
		defer file.Close()
		file.Write([]byte("const dogs=\"good\";"))

		// Test
		language, err := getLanguage(fileName)
		if err != nil {
			t.Errorf("Error getting the language of the test file %s\n%s", fileName, err)
		}

		if language != "JavaScript" {
			t.Errorf("Expected: %s, Got: %s", "JavaScript", language)
		}

		// Cleanup
		os.Remove(fileName)
	})

	t.Run("gets the language from the fileLanguageMap if the file has already been read", func(t *testing.T) {
		// Setup
		fileName := "doge.tss"
		fileLanguageMap[fileName] = "TestyScript"

		// Test
		language, err := getLanguage(fileName)
		if err != nil {
			t.Errorf("Error getting the language of the test file %s\n%s", fileName, err)
		}

		if language != "TestyScript" {
			t.Errorf("Expected: %s, Got: %s", "TestyScript", language)
		}
	})
}

func TestCreateAPIRequest(t *testing.T) {
	t.Run("creates an http request to send pulses to the API", func(t *testing.T) {
		// Setup
		apiToken := "it-me"
		pulses := make([]pulse, 0)
		p := pulse{
			Type:                "typing",
			Date:                "1234",
			ProgrammingLanguage: "Typescript",
			Editor:              "Vim",
		}
		pulses = append(pulses, p)

		// Test
		request, err := createAPIRequest(apiToken, pulses)
		if err != nil {
			t.Errorf("Error creating the API request\n%s", err)
		}

		expectedHeader := fmt.Sprintf("Bearer %s", apiToken)
		header := request.Header.Get("Authorization")
		if header != expectedHeader {
			t.Errorf("Expected: %s, Got: %s", expectedHeader, header)
		}
	})
}

func TestBuildPulses(t *testing.T) {
	t.Run("transforms list of incomingEvents to list of pulses", func(t *testing.T) {
		fileLanguageMap["foo/bar/dogs.ts"] = "TypeScript"
		fileLanguageMap["foo/bar/cats.ts"] = "TypeScript"

		incomingEvents := []incomingEvent{
			incomingEvent{
				FilePath:  "foo/bar/dogs.ts",
				EventType: "typing",
				EventDate: 1,
				Editor:    "Vim",
			},
			incomingEvent{
				FilePath:  "foo/bar/cats.ts",
				EventType: "saveFile",
				EventDate: 2,
				Editor:    "Vim",
			},
		}

		result := buildPulses(incomingEvents)

		expected := pulse{
			Type:                incomingEvents[0].EventType,
			Date:                time.Unix(0, incomingEvents[0].EventDate*1_000_000).Format(time.RFC3339Nano),
			ProgrammingLanguage: "TypeScript",
			Editor:              incomingEvents[0].Editor,
		}

		if firstEvent := result[0]; !cmp.Equal(firstEvent, expected) {
			t.Errorf("Expected: %s, Got: %s", expected, firstEvent)
		}

		expected = pulse{
			Type:                incomingEvents[1].EventType,
			Date:                time.Unix(0, incomingEvents[1].EventDate*1_000_000).Format(time.RFC3339Nano),
			ProgrammingLanguage: "TypeScript",
			Editor:              incomingEvents[1].Editor,
		}

		if secondEvent := result[1]; !cmp.Equal(secondEvent, expected) {
			t.Errorf("Expected: %s, Got: %s", expected, secondEvent)
		}
	})
}
