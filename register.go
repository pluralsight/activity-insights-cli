package main

import (
	"fmt"
	"log"
	"os"
	"path/filepath"

	"github.com/gofrs/flock"
	"github.com/google/uuid"
)

func getAPIToken() (string, error) {
	creds, err := fetchCredentials(credentialsFileName)
	if err != nil {
		return "", fmt.Errorf("Error fetching creds in the register user function\n%s", err)
	}
	apiToken := creds.APIToken

	if apiToken == "" {
		home, err := os.UserHomeDir()
		if err != nil {
			return "", fmt.Errorf("Error getting the user's home dir\n%s", err)
		}

		filePath := filepath.Join(home, pluralsightDir, "credentials.lock")
		fileLock := flock.New(filePath)
		locked, err := fileLock.TryLock()
		if err != nil {
			return "", fmt.Errorf("Error tyring to take out a lock on the credentails file\n%s", err)
		}

		if locked {
			defer fileLock.Unlock()
			creds, err := fetchCredentials(credentialsFileName)
			if err != nil {
				return "", fmt.Errorf("Error fetching creds after receiving a file lock\n%s", err)
			}

			apiToken = creds.APIToken

			if apiToken == "" {
				apiToken, err = createAPIToken()
				if err != nil {
					return "", err
				}
			}
		} else {
			log.Println("Another process already has a lock on the credentials file. That process is probably in the process of registering")
			return "", fmt.Errorf("Another proccess already has a lock on the file. Aborting here")
		}
	}

	return apiToken, nil
}

func registerUser() {
	apiToken, err := getAPIToken()
	if err != nil {
		openBrowser("https://app.pluralsight.com/id?redirectTo=https://app.pluralsight.com/activity-insights-beta?error=unsuccessful-registration")
		panicOnError(err, "Unsuccessful registration")
	}
	openBrowser(fmt.Sprintf("https://app.pluralsight.com/id?redirectTo=https://app.pluralsight.com/wsd/api/ps-time/register?apiToken=%s", apiToken))
}

func createAPIToken() (string, error) {
	apiToken, err := uuid.NewRandom()
	if err != nil {
		return "", fmt.Errorf("Error creating the api token\n%s", err)
	}

	file, err := getPluralsightFile(credentialsFileName)
	if err != nil {
		return "", fmt.Errorf("Error opening the credentials.yaml file to write the api token\n%s", err)
	}

	_, err = file.Write([]byte(fmt.Sprintln("api_token:", apiToken)))
	if err != nil {
		return "", fmt.Errorf("Error writing the api token to the credentials file\n%s", err)
	}

	return apiToken.String(), nil
}
