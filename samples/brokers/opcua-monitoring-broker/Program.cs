/* ========================================================================
 * Copyright (c) 2005-2019 The OPC Foundation, Inc. All rights reserved.
 *
 * OPC Foundation MIT License 1.00
 * 
 * Permission is hereby granted, free of charge, to any person
 * obtaining a copy of this software and associated documentation
 * files (the "Software"), to deal in the Software without
 * restriction, including without limitation the rights to use,
 * copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the
 * Software is furnished to do so, subject to the following
 * conditions:
 * 
 * The above copyright notice and this permission notice shall be
 * included in all copies or substantial portions of the Software.
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
 * EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
 * OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
 * NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
 * HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
 * WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
 * FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
 * OTHER DEALINGS IN THE SOFTWARE.
 *
 * The complete license agreement can be found here:
 * http://opcfoundation.org/License/MIT/1.00/
 * ======================================================================*/

using Opc.Ua;
using Opc.Ua.Client;
using Opc.Ua.Configuration;
using System;
using System.IO;
using System.Security.Cryptography.X509Certificates;
using System.Threading;
using System.Threading.Tasks;
using Microsoft.AspNetCore.Hosting;
using Microsoft.Extensions.Hosting;
using Grpc.Core;
using OpcuaNode;

namespace OpcuaNodeMonitoring
{

    public enum ExitCode : int
    {
        Ok = 0,
        ErrorCreateApplication = 0x11,
        ErrorDiscoverEndpoints = 0x12,
        ErrorCreateSession = 0x13,
        ErrorBrowseNamespace = 0x14,
        ErrorCreateSubscription = 0x15,
        ErrorMonitoredItem = 0x16,
        ErrorAddSubscription = 0x17,
        ErrorRunning = 0x18,
        ErrorNoKeepAlive = 0x30,
    };

    public class Program
    {
        // Name of environment variable that holds OPC UA DiscoveryURL
        public const string OpcuaDiscoveryUrlLabel = "OPCUA_DISCOVERY_URL";
        // Name of environment variable that holds the identifier for the OPC UA Node to monitor
        public const string IdentifierLabel = "IDENTIFIER";
        // Name of environment variable that holds the amespaceIndex for the OPC UA Node to monitor
        public const string NamespaceIndexLabel = "NAMESPACE_INDEX";
        // DiscoveryURL of the server with which this OPC UA Client creates a secure connection
        public static string OpcuaServerDiscoveryURL;
        // NodeId for the OPC UA Node to monitor (See https://reference.opcfoundation.org/v104/Core/docs/Part3/8.2.1/). 
        // NamespaceIndex and Identifier are passed to the broker as environment variables. 
        // Identifier's identifierType must be a String. TODO: Support other identifierTypes
        public static NodeId MonitoredNodeId;

        static int Main(string[] args)
        {
            Console.WriteLine(".NET Core OPC UA Console Client Start");

            // Get OPC UA Server DiscoveryURL and store it as a global variable 
            OpcuaServerDiscoveryURL = Environment.GetEnvironmentVariable(OpcuaDiscoveryUrlLabel);
            if (string.IsNullOrEmpty(OpcuaServerDiscoveryURL))
            {
                throw new ArgumentNullException("Unable to get OPC UA endpoint in environment variable {0}", OpcuaDiscoveryUrlLabel);
            }
            string OpcuaNodeIdentifier = Environment.GetEnvironmentVariable(IdentifierLabel);
            if (string.IsNullOrEmpty(OpcuaNodeIdentifier))
            {
                throw new ArgumentNullException("Unable to get OPC UA endpoint in environment variable {0}", IdentifierLabel);
            }
            ushort OpcuaNamespaceIndex = ushort.Parse(Environment.GetEnvironmentVariable(NamespaceIndexLabel));
            MonitoredNodeId = new NodeId(OpcuaNodeIdentifier, OpcuaNamespaceIndex);
            // Require certificate validation instead of automatically accepting certificates from server.
            MonitoringClient client = new MonitoringClient(OpcuaServerDiscoveryURL);
            Task HostBuilderTask = Task.Run(() => CreateHostBuilder(args).Build().Run());
            client.Run();
            return (int)client.ExitCodeValue;
        }
        public static IHostBuilder CreateHostBuilder(string[] args) =>
            Host.CreateDefaultBuilder(args)
        .ConfigureWebHostDefaults(webBuilder =>
        {
            webBuilder.UseStartup<Startup>();
        });
    }

    // gRPC Server which serves the latest value of the monitored Node. It assumes the value is integer type.
    // TODO: support other value types.
    public class OpcuaNodeService : OpcuaNode.OpcuaNode.OpcuaNodeBase
    {
        // Returns latest value of monitored Node
        public override Task<ValueResponse> GetValue(ValueRequest request, ServerCallContext context)
        {
            int value;
            if (MonitoringClient.LatestValue.HasValue)
            {
                value = MonitoringClient.LatestValue.Value;
                Console.WriteLine("Sending a value of {0} for server at DiscoveryURL {1}", value, Program.OpcuaServerDiscoveryURL);
            }
            else
            {
                Console.WriteLine("No value available for server at DiscoveryURL {0}, sending value of 0", Program.OpcuaServerDiscoveryURL);
                value = 0;
            }
            return Task.FromResult(new ValueResponse
            {
                Value = value,
                OpcuaServer = Program.OpcuaServerDiscoveryURL
            });
        }
    }

    // OPC UA client that connects to the OPC UA server at the DiscoveryURL passed to it as an environment variable
    // It uses the credentials mounted 
    public class MonitoringClient
    {
        // Timeout for reconnecting
        readonly TimeSpan ReconnectPeriod = TimeSpan.FromSeconds(10);
        // Run client indefinitely
        readonly TimeSpan ClientRunTime = Timeout.InfiniteTimeSpan;
        // The OPC UA session
        Session Session;
        SessionReconnectHandler ReconnectHandler;
        // DiscoveryURL for OPC UA server
        string DiscoveryURL;
        // Value for automatically accepting the server's credentials
        // May want to add option to configure it in the future
        static readonly bool AutoAccept = false;
        // Object for locking when modifying LatestValue
        static object LatestValueLock = new object();
        // Expected location of credentials mounted as secrets
        private const String ClientPKIPath = "/etc/opcua-certs/client-pki/";
        // Holds the ExitCode
        public ExitCode ExitCodeValue { get; private set; }
        private static int? _latestValue;
        // Latest value of Node to which the the client is subscribed
        public static int? LatestValue
        {
            get { lock (LatestValueLock) { return _latestValue; } }
            private set { lock (LatestValueLock) { _latestValue = value; } }
        }

        // OPC UA client that subscribes to a Node and stores it's latest value
        public MonitoringClient(string _discoveryURL)
        {
            DiscoveryURL = _discoveryURL;
        }

        // Get the subject name of the certificate mounted in the pod, else return substitute name and later use no security.
        private X509Certificate2 GetCertificate(string certificateStorePath)
        {
            DirectoryInfo certificateStore = new DirectoryInfo(certificateStorePath);
            FileInfo[] files = certificateStore.GetFiles("*.der");
            if (files.Length == 0)
            {
                throw new FileNotFoundException("No certificate mounted at path {0}", certificateStorePath);
            }
            else if (files.Length > 1)
            {
                Console.WriteLine("Error: found more than one der certificate in folder {0}. Using first one: {1}", certificateStorePath, files[0]);
            }
            FileInfo clientCertificate = files[0];
            X509Certificate2 certificate = new X509Certificate2(clientCertificate.FullName);
            return certificate;
        }

        // Builds an ApplicationConfiguration for the OPC UA Client, using the credentials mounted as secrets 
        // at `clientPKIPath,` which should already have the OPC UA Server's CA in the trusted folder. 
        // If no credentials were mounted, an insecure connection is made with the OPC UA Server (SecurityPolicy = None).
        private ApplicationConfiguration CreateApplicationConfiguration()
        {
            CertificateIdentifier certificateIdentifier;
            string clientCertificateFolder = ClientPKIPath + "own/certs/";
            try
            {
                X509Certificate2 certificate = GetCertificate(clientCertificateFolder);
                certificateIdentifier = new CertificateIdentifier
                {
                    StoreType = "Directory",
                    StorePath = ClientPKIPath + "own",
                    Certificate = certificate,
                };
            }
            catch (Exception e)
            {
                Console.WriteLine("Exception {0} thrown when trying to use application certificate mounted at {1}. Using no security.", e, clientCertificateFolder);
                certificateIdentifier = new CertificateIdentifier();
            }

            ApplicationConfiguration config = new ApplicationConfiguration()
            {
                // Application name doesn't matter for certificate validation
                ApplicationName = "AkriOPCUABroker",
                ApplicationType = ApplicationType.Client,
                // If ApplicationUri is not specified, one is automatically created with the format "urn:<hostname>:<ApplicationName>"
                SecurityConfiguration = new SecurityConfiguration
                {
                    ApplicationCertificate = certificateIdentifier,
                    TrustedIssuerCertificates = new CertificateTrustList
                    {
                        StoreType = "Directory",
                        StorePath = ClientPKIPath + "issuer",
                    },
                    TrustedPeerCertificates = new CertificateTrustList
                    {
                        StoreType = "Directory",
                        StorePath = ClientPKIPath + "trusted",
                    },
                    RejectedCertificateStore = new CertificateTrustList
                    {
                        StoreType = "Directory",
                        StorePath = ClientPKIPath + "rejected",
                    },
                    NonceLength = 32,
                    AutoAcceptUntrustedCertificates = AutoAccept
                },
                TransportConfigurations = new TransportConfigurationCollection(),
                TransportQuotas = new TransportQuotas { OperationTimeout = (int)TimeSpan.FromMinutes(10).TotalMilliseconds },
                ClientConfiguration = new ClientConfiguration { DefaultSessionTimeout = (int)TimeSpan.FromMinutes(1).TotalMilliseconds }
            };

            return config;
        }

        public void Run()
        {
            try
            {
                CreateAndRunClient().Wait();
            }
            catch (Exception ex)
            {
                Console.WriteLine("Exception: {0}", ex.Message);
                return;
            }

            ManualResetEvent quitEvent = new ManualResetEvent(false);
            Console.CancelKeyPress += (sender, eArgs) =>
            {
                quitEvent.Set();
                eArgs.Cancel = true;
            };

            // wait for timeout or Ctrl-C
            quitEvent.WaitOne(ClientRunTime);

            // return error conditions
            if (Session.KeepAliveStopped)
            {
                ExitCodeValue = ExitCode.ErrorNoKeepAlive;
                return;
            }

            ExitCodeValue = ExitCode.Ok;
        }

        private async Task CreateAndRunClient()
        {
            Console.WriteLine("1 - Create an Application Configuration.");
            ExitCodeValue = ExitCode.ErrorCreateApplication;
            ApplicationConfiguration config = this.CreateApplicationConfiguration();
            await config.Validate(ApplicationType.Client);
            bool haveAppCertificate = config.SecurityConfiguration.ApplicationCertificate.Certificate != null;
            ApplicationInstance application = new ApplicationInstance(config);

            if (haveAppCertificate)
            {
                config.ApplicationUri = X509Utils.GetApplicationUriFromCertificate(config.SecurityConfiguration.ApplicationCertificate.Certificate);
                config.CertificateValidator.CertificateValidation += new CertificateValidationEventHandler(CertificateValidator_CertificateValidation);
            }
            else
            {
                // Check if any certificates were mounted and secure connection was desired
                if (Directory.Exists(ClientPKIPath))
                {
                    throw new ArgumentException("Application certificates passed as secrets could not be used. Make sure application subject name is AkriClient, the certs are in der format, and the private key in pfx format");
                }
                Console.WriteLine("Application certificates not mounted, using unsecure connection with Security Policy None");
            }
            Console.WriteLine("Client is using a certificate with subject " + config.SecurityConfiguration.ApplicationCertificate.SubjectName);
            Console.WriteLine("2 - Discover endpoints of {0}.", DiscoveryURL);
            ExitCodeValue = ExitCode.ErrorDiscoverEndpoints;
            EndpointDescription selectedEndpoint = CoreClientUtils.SelectEndpoint(DiscoveryURL, haveAppCertificate, (int)TimeSpan.FromSeconds(15).TotalMilliseconds);
            Console.WriteLine("    Selected endpoint uses: {0}",
                selectedEndpoint.SecurityPolicyUri.Substring(selectedEndpoint.SecurityPolicyUri.LastIndexOf('#') + 1));

            Console.WriteLine("3 - Create a session with OPC UA server.");
            ExitCodeValue = ExitCode.ErrorCreateSession;
            var endpointConfiguration = EndpointConfiguration.Create(config);
            var endpoint = new ConfiguredEndpoint(null, selectedEndpoint, endpointConfiguration);
            Session = await Session.Create(config, endpoint, false, "Akri Client", (uint)TimeSpan.FromSeconds(60).TotalMilliseconds, new UserIdentity(new AnonymousIdentityToken()), null);

            // Register keep alive handler to monitor the status of the session
            Session.KeepAlive += Client_KeepAlive;

            Console.WriteLine("4 - Browse the OPC UA server namespace.");
            DoBrowse();

            Console.WriteLine("5 - Create a subscription with publishing interval of 1 second.");
            ExitCodeValue = ExitCode.ErrorCreateSubscription;
            var subscription = new Subscription(Session.DefaultSubscription) { PublishingInterval = (int)TimeSpan.FromSeconds(1).TotalMilliseconds };

            Console.WriteLine("6 - Add node {0} to the subscription.", Program.MonitoredNodeId.Identifier);
            ExitCodeValue = ExitCode.ErrorMonitoredItem;
            var monitoredNode = new MonitoredItem(subscription.DefaultItem)
            {
                DisplayName = Program.MonitoredNodeId.Identifier.ToString(),
                StartNodeId = Program.MonitoredNodeId,
            };
            monitoredNode.Notification += OnNotification;
            subscription.AddItem(monitoredNode);

            Console.WriteLine("7 - Add the subscription to the session.");
            // TODO: Find way to detect if passed improper NodeID, since if NodeID is not valid, no error is thrown by server
            ExitCodeValue = ExitCode.ErrorAddSubscription;
            Session.AddSubscription(subscription);
            subscription.Create();

            Console.WriteLine("8 - Running...Press Ctrl-C to exit...");
            ExitCodeValue = ExitCode.ErrorRunning;
        }

        // Browse server address space 
        private void DoBrowse()
        {
            // TODO: Posibly extend this to search for NodeID of a Node given a display name
            ExitCodeValue = ExitCode.ErrorBrowseNamespace;
            ReferenceDescriptionCollection references;
            Byte[] continuationPoint;

            references = Session.FetchReferences(ObjectIds.ObjectsFolder);

            Session.Browse(
                null,
                null,
                ObjectIds.ObjectsFolder,
                0u,
                BrowseDirection.Forward,
                ReferenceTypeIds.HierarchicalReferences,
                true,
                (uint)NodeClass.Variable | (uint)NodeClass.Object | (uint)NodeClass.Method,
                out continuationPoint,
                out references);

            Console.WriteLine(" DisplayName, BrowseName, NodeClass");
            foreach (var rd in references)
            {
                Console.WriteLine(" {0}, {1}, {2} ", rd.DisplayName, rd.BrowseName, rd.NodeClass);
                ReferenceDescriptionCollection nextRefs;
                byte[] nextCp;
                Session.Browse(
                    null,
                    null,
                    ExpandedNodeId.ToNodeId(rd.NodeId, Session.NamespaceUris),
                    0u,
                    BrowseDirection.Forward,
                    ReferenceTypeIds.HierarchicalReferences,
                    true,
                    (uint)NodeClass.Variable | (uint)NodeClass.Object | (uint)NodeClass.Method,
                    out nextCp,
                    out nextRefs);

                foreach (var nextRd in nextRefs)
                {
                    Console.WriteLine("   + {0}, {1}, {2} next", nextRd.DisplayName, nextRd.BrowseName, nextRd.NodeClass);
                }
            }
        }

        // Monitor the health of the session and reconnect if needed
        private void Client_KeepAlive(Session sender, KeepAliveEventArgs e)
        {
            if (e.Status != null && ServiceResult.IsNotGood(e.Status))
            {
                Console.WriteLine("{0} {1}/{2}", e.Status, sender.OutstandingRequestCount, sender.DefunctRequestCount);

                if (ReconnectHandler == null)
                {
                    Console.WriteLine("--- RECONNECTING ---");
                    ReconnectHandler = new SessionReconnectHandler();
                    ReconnectHandler.BeginReconnect(sender, ReconnectPeriod.Milliseconds, Client_ReconnectComplete);
                }
            }
        }

        private void Client_ReconnectComplete(object sender, EventArgs e)
        {
            // ignore callbacks from discarded objects.
            if (!Object.ReferenceEquals(sender, ReconnectHandler))
            {
                return;
            }

            Session = ReconnectHandler.Session;
            ReconnectHandler.Dispose();
            ReconnectHandler = null;

            Console.WriteLine("--- RECONNECTED ---");
        }

        // Change latestValue when receive an updated value from the server
        private void OnNotification(MonitoredItem item, MonitoredItemNotificationEventArgs e)
        {
            foreach (var value in item.DequeueValues())
            {
                Console.WriteLine("{0}: {1}, {2}, {3}", item.DisplayName, value.Value, value.SourceTimestamp, value.StatusCode);
                // OPC PLC server demo uses uint so need extra casting
                MonitoringClient.LatestValue = (int)(uint)value.Value;
            }
        }

        private static void CertificateValidator_CertificateValidation(CertificateValidator validator, CertificateValidationEventArgs e)
        {
            if (e.Error.StatusCode == StatusCodes.BadCertificateUntrusted)
            {
                e.Accept = AutoAccept;
                if (AutoAccept)
                {
                    Console.WriteLine("Accepted Certificate: {0}", e.Certificate.Subject);
                }
                else
                {
                    Console.WriteLine("Rejected Certificate: {0}", e.Certificate.Subject);
                }
            }
        }

    }
}
