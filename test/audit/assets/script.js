console.log("p2p lab started");

var svg = d3.select("#map");
svg.append("circle")
  .attr("cx", 2).attr("cy", 2).attr("r", 40).style("fill", "blue");
svg.append("circle")
  .attr("cx", 140).attr("cy", 70).attr("r", 40).style("fill", "red");
svg.append("circle")
  .attr("cx", 300).attr("cy", 100).attr("r", 40).style("fill", "green");

var socket = new WebSocket("ws://" + location.host + "/stream");

socket.onopen = (event) => {
  console.log("socket opened ", event);
};

socket.onmessage = (event) => {
  console.log("socket message", event);
}

