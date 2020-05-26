package main

import (
	"fmt"
	"io/ioutil"
	"os"
	"path/filepath"
	"testing"
)

func TestGetAPIToken(t *testing.T) {
	home, err := os.UserHomeDir()
	if err != nil {
		t.Errorf("Error getting the users home directory in the getPluralsightFile Test")
	}
	pluralsightDir = ".notpluralsight"
	credentialsFileName = "notcreds.yaml"

	t.Run("fetches the api token when the credentials file already exists", func(t *testing.T) {
		// Setup
		os.MkdirAll(filepath.Join(home, pluralsightDir), 0777)
		createdFile, _ := os.Create(filepath.Join(home, pluralsightDir, credentialsFileName))
		defer createdFile.Close()
		createdFile.Write([]byte("api_token: not-a-real-token"))

		// Test
		apiToken, err := getAPIToken()
		if err != nil {
			t.Errorf("Error getting the API token \n%s", err)
		}

		if apiToken != "not-a-real-token" {
			t.Errorf("Expected: %s, Got: %s", "not-a-real-token", apiToken)
		}

		// Cleanup
		os.RemoveAll(filepath.Join(home, pluralsightDir))
	})

	t.Run("creates an api token when the credentials file does not exist", func(t *testing.T) {
		// Test
		apiToken, err := getAPIToken()
		if err != nil {
			t.Errorf("Error getting the API token \n%s", err)
		}

		if apiToken == "" {
			t.Errorf("API token has not been created or written to the file")
		}

		file, err := os.Open(filepath.Join(home, pluralsightDir, credentialsFileName))
		defer file.Close()

		if err != nil {
			t.Errorf("Unable to open the credentials file %s", credentialsFileName)
		}

		bytes, err := ioutil.ReadAll(file)
		expectedAPIToken := fmt.Sprintf("api_token: %s\n", apiToken)

		if string(bytes) != expectedAPIToken {
			t.Errorf("Expected: %s, Got: %s", expectedAPIToken, string(bytes))
		}

		// Cleanup
		os.RemoveAll(filepath.Join(home, pluralsightDir))
	})
}
