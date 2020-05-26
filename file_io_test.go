package main

import (
	"io/ioutil"
	"log"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestGetPluralsightFile(t *testing.T) {
	home, err := os.UserHomeDir()
	if err != nil {
		t.Errorf("Error getting the users home directory in the getPluralsightFile Test")
	}
	pluralsightDir = ".notpluralsight"
	testFileName := "notcreds.yaml"

	t.Run("it will return a file handle to the file in the pluralsight directory", func(t *testing.T) {
		// Setup
		os.MkdirAll(filepath.Join(home, pluralsightDir), 0777)
		createdFile, _ := os.Create(filepath.Join(home, pluralsightDir, testFileName))
		defer createdFile.Close()

		// Test
		file, err := getPluralsightFile(testFileName)
		if err != nil {
			t.Errorf("Error getting pluralsight file: %s", testFileName)
		}
		defer file.Close()

		info, _ := file.Stat()
		if info.Name() != testFileName {
			t.Errorf("Expected getPluralsightFile to get existing file %s", testFileName)
		}

		// Cleanup
		os.RemoveAll(filepath.Join(home, pluralsightDir))
	})

	t.Run("it will create the pluralsight directory if its not there", func(t *testing.T) {
		// Test
		file, err := getPluralsightFile(testFileName)
		if err != nil {
			t.Errorf("Error getting pluralsight file: %s", testFileName)
		}
		defer file.Close()

		pluralsightDirPath := filepath.Join(home, pluralsightDir)
		dir, err := os.Open(pluralsightDirPath)
		dirInfo, _ := dir.Stat()
		if err != nil {
			t.Errorf("Expected getPluralsightFile to create the pluralsight dir if not there")
		}

		if !dirInfo.IsDir() {
			t.Errorf("Expected to it to create a pluralsight directory")
		}

		if dir.Name() != pluralsightDirPath {
			t.Errorf("Wrong directory name. Expected: %s, Got: %s", pluralsightDirPath, dir.Name())
		}

		info, _ := file.Stat()
		if info.Name() != testFileName {
			t.Errorf("Expected getPluralsightFile to get existing file %s", testFileName)
		}

		// Cleanup
		os.RemoveAll(filepath.Join(home, pluralsightDir))
	})
}

func TestSetLogFile(t *testing.T) {
	home, err := os.UserHomeDir()
	if err != nil {
		t.Errorf("Error getting the users home directory is SetLogFile Test %s", err)
	}
	pluralsightDir = ".notpluralsight"
	logFileName := "not-ps-time.logs"

	t.Run("it will set the output of the logger to go to the log file", func(t *testing.T) {
		file := setLogFile(logFileName)
		defer file.Close()

		log.Println("Testing")
		file.Seek(0, 0)

		content, _ := ioutil.ReadAll(file)

		if !strings.Contains(string(content), "Testing") {
			t.Errorf("Failed to set the log file. Expected log file to contain \"Testing\", Got: %s", content)
		}
	})

	os.RemoveAll(filepath.Join(home, pluralsightDir))
}

func TestFetchCreds(t *testing.T) {
	home, err := os.UserHomeDir()
	if err != nil {
		t.Errorf("Error getting the users home directory is SetLogFile Test %s", err)
	}
	pluralsightDir = ".notpluralsight"
	fakeCredsFileName := "not-creds.yaml"

	os.MkdirAll(filepath.Join(home, pluralsightDir), 0777)
	createdFile, _ := os.Create(filepath.Join(home, pluralsightDir, fakeCredsFileName))
	defer createdFile.Close()
	createdFile.Write([]byte("api_token: not-a-real-token"))

	t.Run("it will return the credentails from the credentials file", func(t *testing.T) {
		creds, _ := fetchCredentials(fakeCredsFileName)
		if creds.APIToken != "not-a-real-token" {
			t.Errorf("Expected: %s, Got: %s", "not-a-real-token", creds.APIToken)
		}
	})

	os.RemoveAll(filepath.Join(home, pluralsightDir))
}
