using System;
using System.IO;
using System.Net;
using Opc.Ua;
using System.Security.Cryptography.X509Certificates;
using System.Collections.Generic;

namespace opcua_certificate_generator
{
    class Program
    {
        static void Main(string[] args)
        {
            const string storeType = "Directory";

            // Make sure a path, num certs, and at least 1 IP address have been provided
            if (args.Length < 3)
            {
                Console.WriteLine("Please enter an absolute path, number of server certificates to create, and IP addresses for each of the servers");
                return;
            }

            // Parse the command line arguments
            string storePath = args[0];
            int numServers = int.Parse(args[1]);
            if (numServers <= 0)
            {
                Console.WriteLine("Please enter a number greater than 0 for number of server certificates to create");
                return;
            }
            if (args.Length != numServers + 2)
            {
                Console.WriteLine("Please provide one IP address for each server. You requested {0} server certificates but provided only {1} IP addresses", numServers, args.Length - 2);
                return;
            }
            string[] ipAddresses = new string[numServers];
            for (int i = 0; i < ipAddresses.Length; i++)
            {
                IPAddress ip;
                bool isValidIP = IPAddress.TryParse(args[i + 2], out ip);
                if (!isValidIP)
                {
                    Console.WriteLine("Please provide a list of valid IP addresses. IP address {0} could not be parsed.", args[i + 2]);
                    return;
                }
                ipAddresses[i] = ip.ToString();
            }

            // Create CA certificate
            string caStorePath = Path.Combine(storePath, "ca");
            X509Certificate2 certificateAuthorityCert = CreateCACert(storeType, caStorePath);
            CreateCACRL(caStorePath, certificateAuthorityCert);

            // Create server certificates 
            for (int i = 0; i < ipAddresses.Length; i++)
            {
                CreateServerCertificate(storeType, storePath, certificateAuthorityCert, caStorePath, i, ipAddresses[i]);
            }

            // Create a client certificate
            CreateClientCertificate(storeType, storePath, certificateAuthorityCert, caStorePath);
            Console.WriteLine("Finished creating certificates. They should be at " + storePath);
        }

        // Create a certificate store for a Server
        static void CreateServerCertificate(string storeType, string storePath, X509Certificate2 caCert, string caStorePath, int serverNumber, string ipAddress)
        {
            ushort keySize = 2048;
            DateTime startTime = DateTime.UtcNow - TimeSpan.FromDays(1);
            ushort lifetimeInMonths = 6;
            ushort hashSizeInBits = CertificateFactory.DefaultHashSize;
            bool isCA = false;
            IList<String> domainNames = new[] { ipAddress };
            string applicationUri = "urn:SomeServer" + serverNumber;
            string applicationName = "SomeServer" + serverNumber;
            string serverStorePath = Path.Combine(storePath, applicationName);
            Console.WriteLine("Creating a certificate for server {0} with IP address {1}", applicationName, ipAddress);
            X509Certificate2 certificateAuthorityCert = CertificateFactory.CreateCertificate(storeType, serverStorePath, null, applicationUri, applicationName, null, domainNames, keySize, startTime, lifetimeInMonths, hashSizeInBits, isCA, caCert);
            RearrangeCertificateStore(serverStorePath, caStorePath);
        }

        // Create a certificate store for the Akri Broker
        static void CreateClientCertificate(string storeType, string storePath, X509Certificate2 caCert, string caStorePath)
        {
            Console.WriteLine("Creating a certificate for the Akri Broker");
            ushort keySize = 2048;
            DateTime startTime = DateTime.UtcNow - TimeSpan.FromDays(1);
            ushort lifetimeInMonths = 6;
            ushort hashSizeInBits = CertificateFactory.DefaultHashSize;
            bool isCA = false;
            string applicationUri = "urn:AkriBroker";
            string applicationName = "AkriBroker";
            string clientStorePath = Path.Combine(storePath, applicationName);
            CertificateFactory.CreateCertificate(storeType, clientStorePath, null, applicationUri, applicationName, null, null, keySize, startTime, lifetimeInMonths, hashSizeInBits, isCA, caCert);
            RearrangeCertificateStore(clientStorePath, caStorePath);
        }

        // Rearrange each Server and Client's certificate store so that it has the format
        // applicationName
        // // own
        // // // certs
        // // // private
        // // trusted
        // // // certs
        // // // crl
        // with its own public cert in applicationName/own/certs,
        // its own private key in applicationName/own/private,
        // and CA cert and crl in applicationName/trusted
        static void RearrangeCertificateStore(string applicationStorePath, string caStorePath)
        {
            // Create an own directory for this server's certificates
            string ownFolderPath = Path.Combine(applicationStorePath, "own");
            DirectoryCopy(applicationStorePath, ownFolderPath, true);
            // Delete outer certs and private folder
            Directory.Delete(Path.Combine(applicationStorePath, "certs"), true);
            Directory.Delete(Path.Combine(applicationStorePath, "private"), true);

            // Move CA cert and CRL to trusted folder
            string trustedFolderPath = Path.Combine(applicationStorePath, "trusted");
            DirectoryCopy(Path.Combine(caStorePath, "certs"), Path.Combine(trustedFolderPath, "certs"), true);
            DirectoryCopy(Path.Combine(caStorePath, "crl"), Path.Combine(trustedFolderPath, "crl"), true);
        }

        // Copies files from one directory to the other
        // Copied from https://docs.microsoft.com/en-us/dotnet/standard/io/how-to-copy-directories on 9/23/2020
        private static void DirectoryCopy(string sourceDirName, string destDirName, bool copySubDirs)
        {
            // Get the subdirectories for the specified directory.
            DirectoryInfo dir = new DirectoryInfo(sourceDirName);

            if (!dir.Exists)
            {
                throw new DirectoryNotFoundException(
                    "Source directory does not exist or could not be found: "
                    + sourceDirName);
            }

            DirectoryInfo[] dirs = dir.GetDirectories();

            // If the destination directory doesn't exist, create it.       
            Directory.CreateDirectory(destDirName);

            // Get the files in the directory and copy them to the new location.
            FileInfo[] files = dir.GetFiles();
            foreach (FileInfo file in files)
            {
                string temppath = Path.Combine(destDirName, file.Name);
                file.CopyTo(temppath, false);
            }

            // If copying subdirectories, copy them and their contents to new location.
            if (copySubDirs)
            {
                foreach (DirectoryInfo subdir in dirs)
                {
                    string temppath = Path.Combine(destDirName, subdir.Name);
                    DirectoryCopy(subdir.FullName, temppath, copySubDirs);
                }
            }
        }

        // Create a CA cert
        static X509Certificate2 CreateCACert(string storeType, string storePath)
        {
            string applicationUri = "urn:SomeCA";
            string applicationName = "SomeCA";
            Console.WriteLine("Creating a CA with name " + applicationName);
            ushort keySize = 2048;
            DateTime startTime = DateTime.UtcNow - TimeSpan.FromDays(1);
            ushort lifetimeInMonths = 6;
            ushort hashSizeInBits = CertificateFactory.DefaultHashSize;
            bool isCA = true;
            X509Certificate2 certificateAuthorityCert = CertificateFactory.CreateCertificate(storeType, storePath, null, applicationUri, applicationName, null, null, keySize, startTime, lifetimeInMonths, hashSizeInBits, isCA, null);
            return certificateAuthorityCert;
        }

        // Create an empty CRL 
        static void CreateCACRL(string storePath, X509Certificate2 caCert)
        {
            Console.WriteLine("Creating an empty certificate revocation list (CRL) for the CA");
            using (ICertificateStore store = CertificateStoreIdentifier.OpenStore(storePath))
            {
                List<X509CRL> caCRL = store.EnumerateCRLs(caCert, false);
                X509CRL updatedCRL = CertificateFactory.RevokeCertificate(caCert, caCRL, null);
                store.AddCRL(updatedCRL);
            }
        }
    }
}
