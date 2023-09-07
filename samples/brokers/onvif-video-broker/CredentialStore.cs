using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;

namespace Akri
{
	class Credential {
		public string Username { get; set; }
		public string Password { get; set; }
	}

	class CredentialData {
		public string Username { get; set; }
		public string Password { get; set; }
		public bool Base64encoded { get; set; }
	}

	class CredentialRefData {
		public string Username_ref { get; set; }
		public string Password_ref { get; set; }
	}

	class CredentialStore {
		private readonly string secretDirectory;
		private readonly string configMapDirectory;

		public CredentialStore(string secretDirectory, string configMapDirectory) {
			this.secretDirectory = secretDirectory;
			this.configMapDirectory = configMapDirectory;
		}

		public Credential Get(string id) {
			if (String.IsNullOrEmpty(this.secretDirectory)) {
				return null;
			}

			Credential defaultCredential = null;
			// get credential from secrets
			var (credential, isDefault) = GetCredentialFromSecret(id);
			if ((credential != null) && !isDefault) {
				return credential;
			}
			defaultCredential ??= credential;

			// get credential from credential ref list
			(credential, isDefault) = GetCredentialFromCredentialRefList(id);
			if ((credential != null) && !isDefault) {
				return credential;
			}
			defaultCredential ??= credential;

			// get credential from credential list
			(credential, isDefault) = GetCredentialFromCredentialList(id);
			if ((credential != null) && !isDefault) {
				return credential;
			}
			defaultCredential ??= credential;

			return defaultCredential;
		}

		private (Credential, bool) GetCredentialFromCredentialRefList(string id) {
			var credentialRefList = GetListContent<List<string>>("device_credential_ref_list");
			if (credentialRefList == null) {
				return (null, false);
			}

			var allRefDictionary = new Dictionary<string, CredentialRefData>();
			foreach (var refList in credentialRefList) {
				var refEntries = GetListContent<Dictionary<string, CredentialRefData>>(refList);
				if (refEntries != null) {
					refEntries.ToList().ForEach(x => allRefDictionary[x.Key] = x.Value);
				}
			}

			var isDefault = false;
			if (!allRefDictionary.TryGetValue(id, out CredentialRefData credentialRefData)) {
				if (!allRefDictionary.TryGetValue("default", out credentialRefData)) {
					return (null, false);
				}
				isDefault = true;
			}
			Credential credential = null;
			string username = ReadStringFromFile(this.secretDirectory, credentialRefData.Username_ref);
			if (!String.IsNullOrEmpty(username)) {
				string password = ReadStringFromFile(this.secretDirectory, credentialRefData.Password_ref);
				credential = new Credential() {
					Username = username,
					Password = password
				};
			}

			return (credential, isDefault);
		}

		private (Credential, bool) GetCredentialFromCredentialList(string id) {
			var credentialList = GetListContent<List<string>>("device_credential_list");
			if (credentialList == null) {
				return (null, false);
			}

			var allCredentialDictionary = new Dictionary<string, CredentialData>();
			foreach (var refList in credentialList) {
				var credentialRef = ReadStringFromFile(this.secretDirectory, refList);
				if (!String.IsNullOrEmpty(credentialRef)) {
					var result = Deserialize<Dictionary<string, CredentialData>>(credentialRef);
					if (result != null) {
						result.ToList().ForEach(x => allCredentialDictionary[x.Key] = x.Value);
					}
				}
			}

			var isDefault = false;
			if (!allCredentialDictionary.TryGetValue(id, out CredentialData credentialData)) {
				if (!allCredentialDictionary.TryGetValue("default", out credentialData)) {
					return (null, false);
				}
				isDefault = true;
			}
			var decodedPassword = credentialData.Password;
			if (credentialData.Base64encoded) {
				byte[] data = Convert.FromBase64String(credentialData.Password);
				decodedPassword = System.Text.Encoding.UTF8.GetString(data);
			}

			Credential credential = new Credential() {
					Username = credentialData.Username,
					Password = decodedPassword
				};

			return (credential, isDefault);
		}

		private (Credential, bool) GetCredentialFromSecret(string id) {
			// Secret uses underscore as key name, replace all dashes with underscore
			id = id.Replace('-', '_');
			var credential = GetCredentialFromSecretById(id);
			if (credential != null) {
				return (credential, false);
			}
			var defaultCredential = GetCredentialFromSecretById("default");
			return (defaultCredential, true);
		}

		private Credential GetCredentialFromSecretById(string id) {
			string usernameFilename = String.Format("username_{0}", id);
			string username = ReadStringFromFile(this.secretDirectory, usernameFilename);
			if (String.IsNullOrEmpty(username)) {
				return null;
			}
			string passwordFilename = String.Format("password_{0}", id);
			string password = ReadStringFromFile(this.secretDirectory, passwordFilename);
			return new Credential() {
				Username = username,
				Password = password
			};
		}

		private TValue GetListContent<TValue>(string listName) {
			string listContent = null;
			if (!String.IsNullOrEmpty(this.configMapDirectory)) {
				listContent = ReadStringFromFile(this.configMapDirectory, listName);
			}
			if (String.IsNullOrEmpty(listContent)) {
				listContent = ReadStringFromFile(this.secretDirectory, listName);
			}
			if (String.IsNullOrEmpty(listContent)) {
				return default(TValue);
			}

			return Deserialize<TValue>(listContent);
		}

		private string ReadStringFromFile(string path, string filename) {
			string fileFullPath = Path.Combine(path, filename);
			try {
				return File.ReadAllText(fileFullPath);
			} catch (Exception) {
				return null;
			}
		}

		private TValue Deserialize<TValue>(string jsonString)
		{
			try {
				var options = new JsonSerializerOptions()
					{
						PropertyNamingPolicy = JsonNamingPolicy.CamelCase
					};
				return JsonSerializer.Deserialize<TValue>(jsonString, options);
			} catch (Exception) {
				return default(TValue);
			}
		}
	}
}
